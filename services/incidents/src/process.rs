//! Applies one check result: stores it, moves the monitor's state and opens or resolves its
//! incident, all in one transaction.
//!
//! The `monitor_status` row is locked for the duration, so results of the same monitor are
//! applied one at a time even with several replicas; the partial unique index on `incidents`
//! is the last line of defence against a second open incident. A result older than the last
//! one applied (a redelivery that arrived late) is stored but does not move the state.

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use taut_model::MonitorKind;
use taut_model::events::{CheckCompleted, IncidentChanged, subjects};
use uuid::Uuid;

use crate::results;
use crate::state::{self, Health, Observation, State, Thresholds, Transition};

/// The monitor columns the incident service reads.
struct MonitorRow {
    name: String,
    kind: MonitorKind,
    interval_s: i32,
    confirmation_period_s: i32,
    recovery_period_s: i32,
    active: bool,
}

/// What applying a result produced.
#[derive(Debug, Default)]
pub struct Applied {
    /// The result was new and stored.
    pub recorded: bool,
    /// Incident event to publish, with its subject.
    pub event: Option<(&'static str, IncidentChanged)>,
}

/// Applies `result`; see the module documentation.
pub async fn apply_result(pool: &PgPool, result: &CheckCompleted) -> Result<Applied, sqlx::Error> {
    let mut tx = pool.begin().await?;
    let monitor = sqlx::query_as!(
        MonitorRow,
        r#"
        SELECT name, kind AS "kind: MonitorKind", interval_s, confirmation_period_s,
               recovery_period_s, active
        FROM monitors
        WHERE id = $1 AND tenant_id = $2
        "#,
        result.monitor_id,
        result.tenant_id,
    )
    .fetch_optional(&mut *tx)
    .await?;
    let Some(monitor) = monitor else {
        return Ok(Applied::default());
    };
    if !results::insert(&mut tx, result).await? {
        return Ok(Applied::default());
    }
    if !monitor.active {
        // A check that was in flight when the monitor was paused: keep it, ignore it.
        tx.commit().await?;
        return Ok(Applied {
            recorded: true,
            event: None,
        });
    }

    let thresholds = Thresholds::new(
        monitor.kind,
        monitor.interval_s,
        monitor.confirmation_period_s,
        monitor.recovery_period_s,
    );
    let Some(transition) = advance(&mut tx, result, thresholds).await? else {
        tx.commit().await?;
        return Ok(Applied {
            recorded: true,
            event: None,
        });
    };

    let event = match transition {
        Transition::None => None,
        Transition::Open { started_at, cause } => {
            open(&mut tx, result, &monitor.name, started_at, cause).await?
        }
        Transition::Resolve { resolved_at } => {
            resolve(&mut tx, result, &monitor.name, resolved_at).await?
        }
    };
    tx.commit().await?;
    Ok(Applied {
        recorded: true,
        event,
    })
}

async fn open(
    tx: &mut sqlx::PgConnection,
    result: &CheckCompleted,
    monitor_name: &str,
    started_at: DateTime<Utc>,
    cause: String,
) -> Result<Option<(&'static str, IncidentChanged)>, sqlx::Error> {
    let id = Uuid::now_v7();
    let inserted = sqlx::query!(
        r#"
        INSERT INTO incidents (id, tenant_id, monitor_id, started_at, cause)
        VALUES ($1, $2, $3, $4, $5)
        ON CONFLICT (monitor_id) WHERE resolved_at IS NULL DO NOTHING
        "#,
        id,
        result.tenant_id,
        result.monitor_id,
        started_at,
        cause,
    )
    .execute(tx)
    .await?;
    if inserted.rows_affected() == 0 {
        return Ok(None);
    }
    Ok(Some((
        subjects::INCIDENTS_OPENED,
        IncidentChanged {
            trace_id: result.trace_id.clone(),
            incident_id: id,
            tenant_id: result.tenant_id,
            monitor_id: result.monitor_id,
            monitor_name: monitor_name.to_owned(),
            started_at,
            resolved_at: None,
            cause,
        },
    )))
}

async fn resolve(
    tx: &mut sqlx::PgConnection,
    result: &CheckCompleted,
    monitor_name: &str,
    resolved_at: DateTime<Utc>,
) -> Result<Option<(&'static str, IncidentChanged)>, sqlx::Error> {
    let resolved = sqlx::query!(
        r#"
        UPDATE incidents SET resolved_at = GREATEST(started_at, $2)
        WHERE monitor_id = $1 AND resolved_at IS NULL
        RETURNING id, started_at, resolved_at AS "resolved_at!", cause
        "#,
        result.monitor_id,
        resolved_at,
    )
    .fetch_optional(tx)
    .await?;
    Ok(resolved.map(|incident| {
        (
            subjects::INCIDENTS_RESOLVED,
            IncidentChanged {
                trace_id: result.trace_id.clone(),
                incident_id: incident.id,
                tenant_id: result.tenant_id,
                monitor_id: result.monitor_id,
                monitor_name: monitor_name.to_owned(),
                started_at: incident.started_at,
                resolved_at: Some(incident.resolved_at),
                cause: incident.cause,
            },
        )
    }))
}

/// Locks the monitor's state, applies `result` and stores the new state. `None` when the
/// result is older than the last one applied.
async fn advance(
    tx: &mut sqlx::PgConnection,
    result: &CheckCompleted,
    thresholds: Thresholds,
) -> Result<Option<Transition>, sqlx::Error> {
    sqlx::query!(
        "INSERT INTO monitor_status (monitor_id, tenant_id) VALUES ($1, $2) ON CONFLICT DO NOTHING",
        result.monitor_id,
        result.tenant_id,
    )
    .execute(&mut *tx)
    .await?;
    let row = sqlx::query!(
        r#"
        SELECT status, consecutive_failures, consecutive_successes, failing_since,
               failure_cause, last_checked_at
        FROM monitor_status
        WHERE monitor_id = $1
        FOR UPDATE
        "#,
        result.monitor_id,
    )
    .fetch_one(&mut *tx)
    .await?;
    if row.last_checked_at.is_some_and(|last| last >= result.at) {
        return Ok(None);
    }

    let current = State {
        health: Health::parse(&row.status),
        consecutive_failures: u32::try_from(row.consecutive_failures).unwrap_or(0),
        consecutive_successes: u32::try_from(row.consecutive_successes).unwrap_or(0),
        failing_since: row.failing_since,
        failure_cause: row.failure_cause,
    };
    let observation = Observation {
        ok: result.ok,
        at: result.at,
        error: result.error.as_deref(),
    };
    let (next, transition) = state::apply(&current, &observation, thresholds);

    sqlx::query!(
        r#"
        UPDATE monitor_status
        SET status = $2, consecutive_failures = $3, consecutive_successes = $4,
            failing_since = $5, failure_cause = $6, last_checked_at = $7, updated_at = now()
        WHERE monitor_id = $1
        "#,
        result.monitor_id,
        next.health.as_str(),
        i32::try_from(next.consecutive_failures).unwrap_or(i32::MAX),
        i32::try_from(next.consecutive_successes).unwrap_or(i32::MAX),
        next.failing_since,
        next.failure_cause,
        result.at,
    )
    .execute(&mut *tx)
    .await?;

    Ok(Some(transition))
}

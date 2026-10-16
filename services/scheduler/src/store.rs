//! Database reads of the scheduler. It never writes: monitors belong to the API.

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use taut_model::MonitorKind;
use uuid::Uuid;

use crate::schedule::ScheduledMonitor;

/// Every active monitor.
pub async fn active_monitors(pool: &PgPool) -> Result<Vec<ScheduledMonitor>, sqlx::Error> {
    sqlx::query_as!(
        ScheduledMonitor,
        r#"
        SELECT id, tenant_id, kind AS "kind: MonitorKind", target, interval_s, timeout_s,
               expected_status, keyword, grace_period_s
        FROM monitors
        WHERE active
        "#
    )
    .fetch_all(pool)
    .await
}

/// One monitor, if it exists and is active.
pub async fn active_monitor(
    pool: &PgPool,
    id: Uuid,
) -> Result<Option<ScheduledMonitor>, sqlx::Error> {
    sqlx::query_as!(
        ScheduledMonitor,
        r#"
        SELECT id, tenant_id, kind AS "kind: MonitorKind", target, interval_s, timeout_s,
               expected_status, keyword, grace_period_s
        FROM monitors
        WHERE id = $1 AND active
        "#,
        id
    )
    .fetch_optional(pool)
    .await
}

/// A heartbeat whose last ping is older than its interval plus grace period.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OverdueHeartbeat {
    /// Monitor id.
    pub id: Uuid,
    /// Owner.
    pub tenant_id: Uuid,
    /// Last ping received.
    pub last_ping_at: DateTime<Utc>,
    /// Expected seconds between pings.
    pub interval_s: i32,
    /// Extra seconds of silence tolerated.
    pub grace_period_s: i32,
}

/// Which of `ids` are active heartbeats that missed their deadline. Heartbeats that never
/// pinged stay pending and are not overdue.
pub async fn overdue_heartbeats(
    pool: &PgPool,
    ids: &[Uuid],
) -> Result<Vec<OverdueHeartbeat>, sqlx::Error> {
    sqlx::query_as!(
        OverdueHeartbeat,
        r#"
        SELECT id, tenant_id, last_ping_at AS "last_ping_at!", interval_s, grace_period_s
        FROM monitors
        WHERE id = ANY($1)
          AND active
          AND kind = 'heartbeat'
          AND last_ping_at IS NOT NULL
          AND last_ping_at < now() - make_interval(secs => interval_s + grace_period_s)
        "#,
        ids
    )
    .fetch_all(pool)
    .await
}

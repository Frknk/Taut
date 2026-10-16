//! Incident state machine against real PostgreSQL (and NATS for the published events).

use std::time::Duration;

use chrono::{DateTime, Utc};
use futures::StreamExt;
use sqlx::PgPool;
use taut_bus::Bus;
use taut_config::Settings;
use taut_incidents::process::apply_result;
use taut_model::events::{CheckCompleted, IncidentChanged, subjects};
use uuid::Uuid;

struct Fixture {
    tenant: Uuid,
    monitor: Uuid,
}

async fn fixture(pool: &PgPool, confirmation_s: i32, recovery_s: i32) -> Fixture {
    let tenant = Uuid::now_v7();
    let monitor = Uuid::now_v7();
    sqlx::query("INSERT INTO tenants (id, slug, name) VALUES ($1, $2, 'Acme')")
        .bind(tenant)
        .bind(format!("t-{}", tenant.simple()))
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO monitors (id, tenant_id, name, kind, target, interval_s,
                               confirmation_period_s, recovery_period_s)
         VALUES ($1, $2, 'Sitio', 'http', 'https://example.com/', 60, $3, $4)",
    )
    .bind(monitor)
    .bind(tenant)
    .bind(confirmation_s)
    .bind(recovery_s)
    .execute(pool)
    .await
    .unwrap();
    Fixture { tenant, monitor }
}

fn at(minute: i64) -> DateTime<Utc> {
    DateTime::from_timestamp(1_800_000_000 + minute * 60, 0).unwrap()
}

fn result(fixture: &Fixture, minute: i64, ok: bool) -> CheckCompleted {
    CheckCompleted {
        trace_id: format!("trace-{minute}"),
        monitor_id: fixture.monitor,
        tenant_id: fixture.tenant,
        at: at(minute),
        region: "local".to_owned(),
        ok,
        status: Some(if ok { 200 } else { 503 }),
        latency_ms: Some(50),
        error: (!ok).then(|| format!("status 503 at minute {minute}")),
    }
}

/// Applies results one minute apart; returns the subjects of the events produced.
async fn feed(pool: &PgPool, fixture: &Fixture, from: i64, results: &[bool]) -> Vec<String> {
    let mut events = Vec::new();
    for (offset, ok) in results.iter().enumerate() {
        let minute = from + i64::try_from(offset).unwrap();
        let applied = apply_result(pool, &result(fixture, minute, *ok))
            .await
            .unwrap();
        assert!(applied.recorded);
        if let Some((subject, _)) = applied.event {
            events.push(subject.to_owned());
        }
    }
    events
}

async fn status(pool: &PgPool, monitor: Uuid) -> String {
    sqlx::query_scalar("SELECT status FROM monitor_status WHERE monitor_id = $1")
        .bind(monitor)
        .fetch_one(pool)
        .await
        .unwrap()
}

#[sqlx::test(migrator = "taut_db::MIGRATOR")]
async fn opens_on_the_second_failure_and_resolves_after_two_successes(pool: PgPool) {
    let fixture = fixture(&pool, 60, 120).await;
    assert!(feed(&pool, &fixture, 0, &[true, false]).await.is_empty());
    assert_eq!(status(&pool, fixture.monitor).await, "up");

    assert_eq!(
        feed(&pool, &fixture, 2, &[false]).await,
        ["incidents.opened"]
    );
    assert_eq!(status(&pool, fixture.monitor).await, "down");
    let (started_at, cause): (DateTime<Utc>, String) = sqlx::query_as(
        "SELECT started_at, cause FROM incidents WHERE monitor_id = $1 AND resolved_at IS NULL",
    )
    .bind(fixture.monitor)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        started_at,
        at(1),
        "starts at the first failure of the streak"
    );
    assert_eq!(cause, "status 503 at minute 1");

    assert!(
        feed(&pool, &fixture, 3, &[false, true, false])
            .await
            .is_empty()
    );
    assert_eq!(
        feed(&pool, &fixture, 6, &[true, true]).await,
        ["incidents.resolved"]
    );
    let open: i64 = sqlx::query_scalar("SELECT count(*) FROM incidents WHERE resolved_at IS NULL")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(open, 0);
    assert_eq!(status(&pool, fixture.monitor).await, "up");
}

#[sqlx::test(migrator = "taut_db::MIGRATOR")]
async fn redelivered_and_late_results_do_not_move_the_state(pool: PgPool) {
    let fixture = fixture(&pool, 60, 120).await;
    feed(&pool, &fixture, 0, &[false]).await;
    let duplicate = apply_result(&pool, &result(&fixture, 0, false))
        .await
        .unwrap();
    assert!(!duplicate.recorded, "same monitor, instant and region");

    feed(&pool, &fixture, 5, &[true]).await;
    // A failure from minute 3 arrives after the success of minute 5.
    let late = apply_result(&pool, &result(&fixture, 3, false))
        .await
        .unwrap();
    assert!(late.recorded);
    assert!(late.event.is_none());
    let failures: i32 =
        sqlx::query_scalar("SELECT consecutive_failures FROM monitor_status WHERE monitor_id = $1")
            .bind(fixture.monitor)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(failures, 0);
}

#[sqlx::test(migrator = "taut_db::MIGRATOR")]
async fn paused_monitors_record_results_without_incidents(pool: PgPool) {
    let fixture = fixture(&pool, 0, 0).await;
    sqlx::query("UPDATE monitors SET active = false WHERE id = $1")
        .bind(fixture.monitor)
        .execute(&pool)
        .await
        .unwrap();
    assert!(feed(&pool, &fixture, 0, &[false, false]).await.is_empty());
    let incidents: i64 = sqlx::query_scalar("SELECT count(*) FROM incidents")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(incidents, 0);
}

#[sqlx::test(migrator = "taut_db::MIGRATOR")]
async fn concurrent_failures_open_a_single_incident(pool: PgPool) {
    // Confirmation 0: every failure alone would open an incident.
    let fixture = fixture(&pool, 0, 0).await;
    let tasks: Vec<_> = (0..8)
        .map(|minute| {
            let pool = pool.clone();
            let event = result(&fixture, minute, false);
            tokio::spawn(async move { apply_result(&pool, &event).await.unwrap() })
        })
        .collect();
    let mut opened = 0;
    for task in tasks {
        if task.await.unwrap().event.is_some() {
            opened += 1;
        }
    }
    assert_eq!(opened, 1);
    let incidents: i64 = sqlx::query_scalar("SELECT count(*) FROM incidents WHERE monitor_id = $1")
        .bind(fixture.monitor)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(incidents, 1);
}

#[sqlx::test(migrator = "taut_db::MIGRATOR")]
async fn handle_publishes_the_incident_event(pool: PgPool) {
    let fixture = fixture(&pool, 0, 0).await;
    let settings = Settings::load().expect("settings");
    let bus = Bus::connect(&settings.nats, "incidents-tests")
        .await
        .expect("nats is up (just up)");
    let mut opened = bus
        .client()
        .subscribe(subjects::INCIDENTS_OPENED)
        .await
        .unwrap();

    let handled = taut_incidents::handle(&pool, &bus, &result(&fixture, 0, false)).await;
    assert!(matches!(handled, taut_bus::Handled::Done));

    let event = tokio::time::timeout(Duration::from_secs(5), async {
        while let Some(message) = opened.next().await {
            let event: IncidentChanged = serde_json::from_slice(&message.payload).unwrap();
            if event.monitor_id == fixture.monitor {
                return event;
            }
        }
        panic!("subscription closed");
    })
    .await
    .expect("incident event published");
    assert_eq!(event.monitor_name, "Sitio");
    assert_eq!(event.cause, "status 503 at minute 0");
    assert_eq!(event.trace_id, "trace-0");
    assert!(event.resolved_at.is_none());
}

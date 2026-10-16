//! Scheduler against real PostgreSQL, Redis and NATS (`just up`).

use std::time::Duration;

use chrono::Utc;
use futures::StreamExt;
use sqlx::PgPool;
use taut_bus::Bus;
use taut_config::Settings;
use taut_model::events::{CheckCompleted, CheckRequested, subjects};
use taut_scheduler::leader::LeaderLock;
use taut_scheduler::schedule::Due;
use taut_scheduler::{dispatch, store};
use uuid::Uuid;

async fn tenant(pool: &PgPool) -> Uuid {
    let id = Uuid::now_v7();
    sqlx::query("INSERT INTO tenants (id, slug, name) VALUES ($1, $2, 'Test')")
        .bind(id)
        .bind(format!("t-{}", id.simple()))
        .execute(pool)
        .await
        .expect("tenant inserted");
    id
}

async fn http_monitor(pool: &PgPool, tenant: Uuid, active: bool) -> Uuid {
    let id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO monitors (id, tenant_id, name, kind, target, active)
         VALUES ($1, $2, 'web', 'http', 'https://example.com/', $3)",
    )
    .bind(id)
    .bind(tenant)
    .bind(active)
    .execute(pool)
    .await
    .expect("monitor inserted");
    id
}

async fn heartbeat(pool: &PgPool, tenant: Uuid, last_ping_minutes_ago: Option<i32>) -> Uuid {
    let id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO monitors (id, tenant_id, name, kind, heartbeat_token, interval_s,
                               grace_period_s, last_ping_at)
         VALUES ($1, $2, 'cron', 'heartbeat', $3, 60, 60,
                 now() - make_interval(mins => $4))",
    )
    .bind(id)
    .bind(tenant)
    .bind(id.simple().to_string())
    .bind(last_ping_minutes_ago)
    .execute(pool)
    .await
    .expect("heartbeat inserted");
    id
}

async fn bus() -> Bus {
    let settings = Settings::load().expect("settings");
    Bus::connect(&settings.nats, "scheduler-tests")
        .await
        .expect("nats is up (just up)")
}

#[sqlx::test(migrator = "taut_db::MIGRATOR")]
async fn only_active_monitors_are_scheduled(pool: PgPool) {
    let tenant = tenant(&pool).await;
    let active = http_monitor(&pool, tenant, true).await;
    let paused = http_monitor(&pool, tenant, false).await;

    let monitors = store::active_monitors(&pool).await.unwrap();
    let ids: Vec<_> = monitors.iter().map(|m| m.id).collect();
    assert_eq!(ids, vec![active]);
    assert!(
        store::active_monitor(&pool, paused)
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(
        store::active_monitor(&pool, active)
            .await
            .unwrap()
            .unwrap()
            .target
            .as_deref(),
        Some("https://example.com/")
    );
}

#[sqlx::test(migrator = "taut_db::MIGRATOR")]
async fn heartbeats_are_overdue_after_interval_plus_grace(pool: PgPool) {
    let tenant = tenant(&pool).await;
    let late = heartbeat(&pool, tenant, Some(3)).await;
    let on_time = heartbeat(&pool, tenant, Some(1)).await;
    let never = heartbeat(&pool, tenant, None).await;

    let overdue = store::overdue_heartbeats(&pool, &[late, on_time, never])
        .await
        .unwrap();
    let ids: Vec<_> = overdue.iter().map(|h| h.id).collect();
    assert_eq!(
        ids,
        vec![late],
        "60 s + 60 s grace: 3 min late, 1 min on time"
    );
}

#[sqlx::test(migrator = "taut_db::MIGRATOR")]
async fn dispatch_publishes_requests_and_missed_heartbeats(pool: PgPool) {
    let tenant = tenant(&pool).await;
    let web = http_monitor(&pool, tenant, true).await;
    let cron = heartbeat(&pool, tenant, Some(5)).await;
    let bus = bus().await;
    let mut requested = bus
        .client()
        .subscribe(subjects::CHECKS_REQUESTED)
        .await
        .unwrap();
    let mut completed = bus
        .client()
        .subscribe(subjects::CHECKS_COMPLETED)
        .await
        .unwrap();

    let now = Utc::now();
    let due = store::active_monitors(&pool)
        .await
        .unwrap()
        .into_iter()
        .map(|monitor| Due {
            monitor,
            scheduled_at: now,
        })
        .collect();
    dispatch(&pool, &bus, "test", due).await;

    let request = tokio::time::timeout(Duration::from_secs(5), async {
        while let Some(message) = requested.next().await {
            let event: CheckRequested = serde_json::from_slice(&message.payload).unwrap();
            if event.monitor_id == web {
                return event;
            }
        }
        panic!("subscription closed");
    })
    .await
    .expect("check requested");
    assert_eq!(request.target, "https://example.com/");
    assert_eq!(request.timeout_s, 10);
    assert_eq!(
        request.expires_at - request.scheduled_at,
        chrono::Duration::seconds(60)
    );
    assert_eq!(request.trace_id.len(), 26);

    let missed = tokio::time::timeout(Duration::from_secs(5), async {
        while let Some(message) = completed.next().await {
            let event: CheckCompleted = serde_json::from_slice(&message.payload).unwrap();
            if event.monitor_id == cron {
                return event;
            }
        }
        panic!("subscription closed");
    })
    .await
    .expect("missed heartbeat reported");
    assert!(!missed.ok);
    assert_eq!(missed.region, "test");
    assert!(missed.error.unwrap().starts_with("no ping for"));
}

#[tokio::test]
async fn only_one_replica_holds_the_lock() {
    let settings = Settings::load().expect("settings");
    let client = redis::Client::open(settings.redis.url.as_str()).unwrap();
    let key = format!("lock:test:{}", Uuid::now_v7());
    let ttl = Duration::from_secs(5);
    let mut first = LeaderLock::new(client.get_connection_manager().await.unwrap(), &key, ttl);
    let mut second = LeaderLock::new(client.get_connection_manager().await.unwrap(), &key, ttl);

    assert!(first.acquire().await.unwrap());
    assert!(!second.acquire().await.unwrap());
    assert!(first.renew().await.unwrap());
    assert!(
        !second.renew().await.unwrap(),
        "cannot renew a lock it does not hold"
    );

    second.release().await.unwrap();
    assert!(
        !second.acquire().await.unwrap(),
        "release by a non-owner is a no-op"
    );
    first.release().await.unwrap();
    assert!(second.acquire().await.unwrap());
    second.release().await.unwrap();
}

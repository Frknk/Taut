//! Recording check results against a real PostgreSQL.

use chrono::Utc;
use sqlx::PgPool;
use taut_model::events::CheckCompleted;
use uuid::Uuid;

async fn monitor(pool: &PgPool) -> (Uuid, Uuid) {
    let tenant = Uuid::now_v7();
    let monitor = Uuid::now_v7();
    sqlx::query("INSERT INTO tenants (id, slug, name) VALUES ($1, $2, 'Acme')")
        .bind(tenant)
        .bind(format!("t-{}", tenant.simple()))
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO monitors (id, tenant_id, name, kind, target)
         VALUES ($1, $2, 'web', 'http', 'https://example.com/')",
    )
    .bind(monitor)
    .bind(tenant)
    .execute(pool)
    .await
    .unwrap();
    (tenant, monitor)
}

fn result(tenant: Uuid, monitor: Uuid) -> CheckCompleted {
    CheckCompleted {
        trace_id: "01J00000000000000000000000".to_owned(),
        monitor_id: monitor,
        tenant_id: tenant,
        at: Utc::now(),
        region: "local".to_owned(),
        ok: false,
        status: Some(500),
        latency_ms: Some(42),
        error: Some("status 500 (expected 2xx)".to_owned()),
    }
}

#[sqlx::test(migrator = "taut_db::MIGRATOR")]
async fn results_are_recorded_once(pool: PgPool) {
    let (tenant, monitor) = monitor(&pool).await;
    let event = result(tenant, monitor);
    assert!(
        taut_incidents::results::record(&pool, &event)
            .await
            .unwrap()
    );
    assert!(
        !taut_incidents::results::record(&pool, &event)
            .await
            .unwrap(),
        "a redelivered event is a no-op"
    );

    let (status, error): (Option<i16>, Option<String>) =
        sqlx::query_as("SELECT status, error FROM check_results WHERE monitor_id = $1")
            .bind(monitor)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(status, Some(500));
    assert_eq!(error.as_deref(), Some("status 500 (expected 2xx)"));
}

#[sqlx::test(migrator = "taut_db::MIGRATOR")]
async fn results_of_unknown_monitors_are_skipped(pool: PgPool) {
    let (tenant, _) = monitor(&pool).await;
    let orphan = result(tenant, Uuid::now_v7());
    assert!(
        !taut_incidents::results::record(&pool, &orphan)
            .await
            .unwrap()
    );

    let other_tenant = result(Uuid::now_v7(), monitor(&pool).await.1);
    assert!(
        !taut_incidents::results::record(&pool, &other_tenant)
            .await
            .unwrap(),
        "tenant and monitor must match"
    );
}

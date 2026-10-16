//! Public status page against a real PostgreSQL.

use std::sync::Arc;

use axum::body::Body;
use http::{Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::Value;
use sqlx::PgPool;
use taut_config::Settings;
use tower::ServiceExt;
use uuid::Uuid;

async fn tenant(pool: &PgPool, slug: &str) -> Uuid {
    let id = Uuid::now_v7();
    sqlx::query("INSERT INTO tenants (id, slug, name) VALUES ($1, $2, $3)")
        .bind(id)
        .bind(slug)
        .bind(format!("Tenant {slug}"))
        .execute(pool)
        .await
        .unwrap();
    id
}

async fn monitor(pool: &PgPool, tenant: Uuid, name: &str, target: &str) -> Uuid {
    let id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO monitors (id, tenant_id, name, kind, target) VALUES ($1, $2, $3, 'http', $4)",
    )
    .bind(id)
    .bind(tenant)
    .bind(name)
    .bind(target)
    .execute(pool)
    .await
    .unwrap();
    id
}

async fn results(pool: &PgPool, tenant: Uuid, monitor: Uuid, ok: i32, failed: i32) {
    sqlx::query(
        "INSERT INTO check_results (monitor_id, tenant_id, at, region, ok, error)
         SELECT $1, $2, now() - make_interval(secs => n), 'local', n > $4,
                CASE WHEN n > $4 THEN NULL ELSE 'dns: could not resolve db.internal' END
         FROM generate_series(1, $3 + $4) AS n",
    )
    .bind(monitor)
    .bind(tenant)
    .bind(ok)
    .bind(failed)
    .execute(pool)
    .await
    .unwrap();
}

async fn get(pool: PgPool, path: &str) -> (StatusCode, Option<String>, Value) {
    let settings = Arc::new(Settings::default());
    let telemetry = taut_telemetry::init(taut_status_bff::SERVICE_NAME, &settings.telemetry)
        .expect("telemetry");
    let app = taut_status_bff::app(&settings, pool, telemetry.metrics().clone());
    let response = app
        .oneshot(Request::get(path).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let cache = response
        .headers()
        .get("cache-control")
        .map(|value| value.to_str().unwrap().to_owned());
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, cache, serde_json::from_slice(&bytes).unwrap())
}

#[sqlx::test(migrator = "taut_db::MIGRATOR")]
async fn page_shows_status_bars_and_incidents_without_internals(pool: PgPool) {
    let acme = tenant(&pool, "acme").await;
    let web = monitor(
        &pool,
        acme,
        "Sitio web",
        "https://secret.acme.internal/health",
    )
    .await;
    let db = monitor(&pool, acme, "Base de datos", "https://db.acme.internal").await;
    results(&pool, acme, web, 98, 2).await;
    results(&pool, acme, db, 0, 3).await;
    sqlx::query("INSERT INTO monitor_status (monitor_id, tenant_id, status) VALUES ($1, $3, 'up'), ($2, $3, 'down')")
        .bind(web)
        .bind(db)
        .bind(acme)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO incidents (id, tenant_id, monitor_id, started_at, cause)
         VALUES ($1, $2, $3, now() - interval '3 minutes', 'dns: could not resolve db.internal')",
    )
    .bind(Uuid::now_v7())
    .bind(acme)
    .bind(db)
    .execute(&pool)
    .await
    .unwrap();
    let globex = tenant(&pool, "globex").await;
    monitor(&pool, globex, "Otro tenant", "https://globex.test").await;

    let (status, cache, page) = get(pool, "/s/acme").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(cache.as_deref(), Some("public, max-age=30"));
    assert_eq!(page["tenant"]["name"], "Tenant acme");
    assert_eq!(page["status"], "partial_outage");

    let monitors = page["monitors"].as_array().unwrap();
    assert_eq!(monitors.len(), 2, "only this tenant's monitors");
    assert_eq!(monitors[0]["name"], "Sitio web");
    assert_eq!(monitors[0]["status"], "up");
    assert_eq!(monitors[0]["uptime_90d"], 98.0);
    let days = monitors[0]["days"].as_array().unwrap();
    assert_eq!(days.len(), 90);
    assert_eq!(days[89]["checks"], 100, "today is aggregated in real time");
    assert_eq!(days[0]["uptime"], Value::Null);
    assert_eq!(monitors[1]["status"], "down");

    let incidents = page["incidents"].as_array().unwrap();
    assert_eq!(incidents.len(), 1);
    assert_eq!(incidents[0]["monitor_name"], "Base de datos");
    assert_eq!(incidents[0]["resolved_at"], Value::Null);
    assert!(incidents[0]["duration_s"].as_i64().unwrap() >= 180);

    let text = page.to_string();
    for secret in ["internal", "target", "cause", "dns", "Otro tenant"] {
        assert!(!text.contains(secret), "page leaks {secret}: {text}");
    }
}

#[sqlx::test(migrator = "taut_db::MIGRATOR")]
async fn unknown_slug_is_not_found(pool: PgPool) {
    let (status, _, body) = get(pool, "/s/nobody").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"]["code"], "not_found");
}

#[sqlx::test(migrator = "taut_db::MIGRATOR")]
async fn empty_tenant_is_operational(pool: PgPool) {
    tenant(&pool, "nuevo").await;
    let (status, _, page) = get(pool, "/s/NUEVO").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(page["status"], "operational");
    assert_eq!(page["monitors"], serde_json::json!([]));
}

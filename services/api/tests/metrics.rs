//! `/v1/monitors/{id}/metrics` against a real PostgreSQL.

mod common;

use http::{Method, StatusCode};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

#[sqlx::test(migrator = "taut_db::MIGRATOR")]
async fn reports_uptime_windows_percentiles_and_series(pool: PgPool) {
    let app = common::app(pool.clone()).await;
    let tenant = common::create_tenant(&app, "acme").await;
    let monitor = common::create_monitor(
        &app,
        &tenant,
        json!({ "name": "API", "type": "http", "target": "https://example.com" }),
    )
    .await;
    let id: Uuid = monitor["id"].as_str().unwrap().parse().unwrap();
    // Last hour: 100 checks with latencies 1..=100 ms, all healthy. Three days ago: four
    // checks, two failed. Out of every window: one failure 40 days ago.
    sqlx::query(
        "INSERT INTO check_results (monitor_id, tenant_id, at, region, ok, latency_ms)
         SELECT $1, $2, now() - make_interval(secs => n * 30), 'local', true, n
         FROM generate_series(1, 100) AS n
         UNION ALL
         SELECT $1, $2, now() - interval '3 days' - make_interval(mins => n), 'local', n > 2, 5000
         FROM generate_series(1, 4) AS n
         UNION ALL
         SELECT $1, $2, now() - interval '40 days', 'local', false, NULL",
    )
    .bind(id)
    .bind(tenant.parse::<Uuid>().unwrap())
    .execute(&pool)
    .await
    .unwrap();

    let (status, day) = common::send(
        &app,
        Method::GET,
        &format!("/v1/monitors/{id}/metrics"),
        Some(&tenant),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{day}");
    assert_eq!(day["range"], "24h");
    assert_eq!(day["bucket_s"], 900);
    assert_eq!(day["checks"], 100);
    assert_eq!(day["uptime"]["24h"], 100.0);
    assert_eq!(day["uptime"]["7d"], 100.0 * 102.0 / 104.0);
    assert_eq!(
        day["uptime"]["30d"], day["uptime"]["7d"],
        "the 40-day-old failure is out"
    );
    assert_eq!(day["latency"]["p50_ms"], 50.5);
    assert!((day["latency"]["p95_ms"].as_f64().unwrap() - 95.05).abs() < 1e-9);
    let series = day["series"].as_array().unwrap();
    let total: i64 = series.iter().map(|p| p["checks"].as_i64().unwrap()).sum();
    assert_eq!(total, 100);
    assert!(
        series.len() >= 4,
        "50 minutes of checks span several 15-minute buckets"
    );

    let (_, week) = common::send(
        &app,
        Method::GET,
        &format!("/v1/monitors/{id}/metrics?range=7d"),
        Some(&tenant),
        None,
    )
    .await;
    assert_eq!(week["checks"], 104);
    assert_eq!(week["failures"], 2);
    // The two healthy 5 s checks of three days ago count in the week's mean.
    let mean = week["latency"]["avg_ms"].as_f64().unwrap();
    assert!((mean - 15_050.0 / 102.0).abs() < 1e-9, "{mean}");

    let (status, body) = common::send(
        &app,
        Method::GET,
        &format!("/v1/monitors/{id}/metrics?range=1y"),
        Some(&tenant),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
}

#[sqlx::test(migrator = "taut_db::MIGRATOR")]
async fn monitor_without_results_has_no_figures(pool: PgPool) {
    let app = common::app(pool).await;
    let tenant = common::create_tenant(&app, "acme").await;
    let monitor = common::create_monitor(
        &app,
        &tenant,
        json!({ "name": "Nuevo", "type": "tcp", "target": "db.example.com:5432" }),
    )
    .await;
    let (_, metrics) = common::send(
        &app,
        Method::GET,
        &format!(
            "/v1/monitors/{}/metrics?range=30d",
            monitor["id"].as_str().unwrap()
        ),
        Some(&tenant),
        None,
    )
    .await;
    assert_eq!(metrics["uptime"]["30d"], Value::Null);
    assert_eq!(metrics["latency"]["p50_ms"], Value::Null);
    assert_eq!(metrics["series"], json!([]));
}

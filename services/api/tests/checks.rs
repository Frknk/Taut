//! `/v1/monitors/{id}/checks` and the last check summary.

mod common;

use http::{Method, StatusCode};
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

async fn insert_result(pool: &PgPool, monitor: Uuid, tenant: Uuid, minutes_ago: i32, ok: bool) {
    sqlx::query(
        "INSERT INTO check_results (monitor_id, tenant_id, at, region, ok, status, latency_ms, error)
         VALUES ($1, $2, now() - make_interval(mins => $3), 'local', $4, 200, 100 + $3, $5)",
    )
    .bind(monitor)
    .bind(tenant)
    .bind(minutes_ago)
    .bind(ok)
    .bind((!ok).then_some("timeout after 10 s"))
    .execute(pool)
    .await
    .unwrap();
}

#[sqlx::test(migrator = "taut_db::MIGRATOR")]
async fn lists_recent_results_newest_first(pool: PgPool) {
    let app = common::app(pool.clone()).await;
    let tenant = common::create_tenant(&app, "acme").await;
    let monitor = common::create_monitor(
        &app,
        &tenant,
        json!({ "name": "web", "type": "http", "target": "https://example.com" }),
    )
    .await;
    let id: Uuid = monitor["id"].as_str().unwrap().parse().unwrap();
    let tenant_id: Uuid = tenant.parse().unwrap();
    for minutes_ago in [3, 2, 1] {
        insert_result(&pool, id, tenant_id, minutes_ago, minutes_ago != 1).await;
    }

    let (status, checks) = common::send(
        &app,
        Method::GET,
        &format!("/v1/monitors/{id}/checks?limit=2"),
        Some(&tenant),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let checks = checks.as_array().unwrap();
    assert_eq!(checks.len(), 2);
    assert_eq!(checks[0]["ok"], false);
    assert_eq!(checks[0]["error"], "timeout after 10 s");
    assert_eq!(checks[1]["ok"], true);

    let (_, shown) = common::send(
        &app,
        Method::GET,
        &format!("/v1/monitors/{id}"),
        Some(&tenant),
        None,
    )
    .await;
    assert_eq!(shown["last_check_ok"], false);
    assert_eq!(shown["last_latency_ms"], 101);

    let other = common::create_tenant(&app, "globex").await;
    let (status, _) = common::send(
        &app,
        Method::GET,
        &format!("/v1/monitors/{id}/checks"),
        Some(&other),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[sqlx::test(migrator = "taut_db::MIGRATOR")]
async fn deleting_a_monitor_deletes_its_results(pool: PgPool) {
    let app = common::app(pool.clone()).await;
    let tenant = common::create_tenant(&app, "acme").await;
    let monitor = common::create_monitor(
        &app,
        &tenant,
        json!({ "name": "web", "type": "http", "target": "https://example.com" }),
    )
    .await;
    let id: Uuid = monitor["id"].as_str().unwrap().parse().unwrap();
    insert_result(&pool, id, tenant.parse().unwrap(), 1, true).await;

    let (status, _) = common::send(
        &app,
        Method::DELETE,
        &format!("/v1/monitors/{id}"),
        Some(&tenant),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let left: i64 = sqlx::query_scalar("SELECT count(*) FROM check_results WHERE monitor_id = $1")
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(left, 0);
}

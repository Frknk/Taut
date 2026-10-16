//! `/v1/monitors` against a real PostgreSQL (needs `DATABASE_URL`, see `.env.example`).

mod common;

use std::sync::Arc;

use http::{Method, StatusCode};
use serde_json::{Value, json};
use sqlx::PgPool;
use taut_config::Settings;

fn http_monitor(name: &str) -> Value {
    json!({ "name": name, "type": "http", "target": "https://example.com/health" })
}

#[sqlx::test(migrator = "taut_db::MIGRATOR")]
async fn requests_need_a_token_and_one_of_my_tenants(pool: PgPool) {
    let app = common::app(pool).await;
    let (status, body) =
        common::send_raw(&app, Method::GET, "/v1/monitors", None, None, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["error"]["code"], "unauthorized");
    for tenant in [None, Some("not-a-uuid")] {
        let (status, body) = common::send(&app, Method::GET, "/v1/monitors", tenant, None).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{tenant:?}");
        assert_eq!(body["error"]["code"], "bad_request");
    }
    let (status, body) = common::send(
        &app,
        Method::GET,
        "/v1/monitors",
        Some("0192b4a8-0000-7000-8000-000000000000"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error"]["code"], "not_a_member");
}

#[sqlx::test(migrator = "taut_db::MIGRATOR")]
async fn creates_an_http_monitor_with_defaults(pool: PgPool) {
    let app = common::app(pool).await;
    let tenant = common::create_tenant(&app, "acme").await;
    let monitor = common::create_monitor(&app, &tenant, http_monitor("Sitio")).await;

    assert_eq!(monitor["name"], "Sitio");
    assert_eq!(monitor["type"], "http");
    assert_eq!(monitor["tenant_id"], tenant.as_str());
    assert_eq!(monitor["interval_s"], 60);
    assert_eq!(monitor["timeout_s"], 10);
    assert_eq!(monitor["confirmation_period_s"], 60);
    assert_eq!(monitor["recovery_period_s"], 120);
    assert_eq!(monitor["expected_status"], Value::Null);
    assert_eq!(monitor["heartbeat_token"], Value::Null);
    assert_eq!(monitor["active"], true);
    assert_eq!(monitor["status"], "pending");

    let (status, list) = common::send(&app, Method::GET, "/v1/monitors", Some(&tenant), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(list.as_array().unwrap().len(), 1);
}

#[sqlx::test(migrator = "taut_db::MIGRATOR")]
async fn rejects_invalid_monitors(pool: PgPool) {
    let app = common::app(pool).await;
    let tenant = common::create_tenant(&app, "acme").await;
    for (body, message) in [
        (
            json!({ "name": "x", "type": "http", "target": "https://example.com", "interval_s": 45 }),
            "interval_s must be one of",
        ),
        (
            json!({ "name": "x", "type": "http", "target": "https://example.com", "interval_s": 30, "timeout_s": 30 }),
            "timeout_s must be shorter",
        ),
        (
            json!({ "type": "http", "target": "https://example.com" }),
            "missing field `name`",
        ),
        (
            json!({ "name": "x", "type": "http", "target": "http://localhost:8080" }),
            "public address",
        ),
        (
            json!({ "name": "x", "type": "ping", "target": "example.com" }),
            "unknown variant",
        ),
    ] {
        let (status, response) = common::send(
            &app,
            Method::POST,
            "/v1/monitors",
            Some(&tenant),
            Some(body.clone()),
        )
        .await;
        assert_eq!(status.as_u16() / 100, 4, "{body} → {response}");
        let text = response["error"]["message"].as_str().unwrap();
        assert!(text.contains(message), "{body} → {text}");
    }
}

#[sqlx::test(migrator = "taut_db::MIGRATOR")]
async fn private_targets_are_allowed_when_configured(pool: PgPool) {
    let mut settings = Settings::default();
    settings.checks.allow_private_targets = true;
    let app = common::app_with(Arc::new(settings), pool).await;
    let tenant = common::create_tenant(&app, "acme").await;
    let monitor = common::create_monitor(
        &app,
        &tenant,
        json!({ "name": "Local", "type": "tcp", "target": "localhost:5432" }),
    )
    .await;
    assert_eq!(monitor["target"], "localhost:5432");
}

#[sqlx::test(migrator = "taut_db::MIGRATOR")]
async fn heartbeat_monitors_get_a_token(pool: PgPool) {
    let app = common::app(pool).await;
    let tenant = common::create_tenant(&app, "acme").await;
    let monitor = common::create_monitor(
        &app,
        &tenant,
        json!({ "name": "Backup nocturno", "type": "heartbeat", "interval_s": 300 }),
    )
    .await;
    assert_eq!(monitor["target"], Value::Null);
    assert_eq!(monitor["heartbeat_token"].as_str().map(str::len), Some(32));
    assert_eq!(monitor["grace_period_s"], 60);
}

#[sqlx::test(migrator = "taut_db::MIGRATOR")]
async fn updates_pauses_resumes_and_deletes(pool: PgPool) {
    let app = common::app(pool).await;
    let tenant = common::create_tenant(&app, "acme").await;
    let monitor = common::create_monitor(&app, &tenant, http_monitor("Sitio")).await;
    let path = format!("/v1/monitors/{}", monitor["id"].as_str().unwrap());

    let (status, updated) = common::send(
        &app,
        Method::PUT,
        &path,
        Some(&tenant),
        Some(json!({
            "name": "Sitio principal",
            "type": "keyword",
            "target": "https://example.com",
            "keyword": "Welcome",
        })),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "type is immutable: {updated}"
    );

    let (status, updated) = common::send(
        &app,
        Method::PUT,
        &path,
        Some(&tenant),
        Some(json!({
            "name": "Sitio principal",
            "type": "http",
            "target": "https://example.com/status",
            "interval_s": 30,
            "timeout_s": 5,
            "expected_status": 204,
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{updated}");
    assert_eq!(updated["name"], "Sitio principal");
    assert_eq!(updated["interval_s"], 30);
    assert_eq!(updated["expected_status"], 204);

    let (status, paused) = common::send(
        &app,
        Method::POST,
        &format!("{path}/pause"),
        Some(&tenant),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(paused["active"], false);
    assert_eq!(paused["status"], "paused");

    let (status, resumed) = common::send(
        &app,
        Method::POST,
        &format!("{path}/resume"),
        Some(&tenant),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(resumed["active"], true);
    assert_eq!(resumed["status"], "pending");

    let (status, _) = common::send(&app, Method::DELETE, &path, Some(&tenant), None).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _) = common::send(&app, Method::GET, &path, Some(&tenant), None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[sqlx::test(migrator = "taut_db::MIGRATOR")]
async fn tenants_cannot_see_each_other(pool: PgPool) {
    let app = common::app(pool).await;
    let acme = common::create_tenant(&app, "acme").await;
    let globex = common::create_tenant(&app, "globex").await;
    let monitor = common::create_monitor(&app, &acme, http_monitor("Sitio")).await;
    let path = format!("/v1/monitors/{}", monitor["id"].as_str().unwrap());

    let (status, list) = common::send(&app, Method::GET, "/v1/monitors", Some(&globex), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(list, json!([]));
    for method in [Method::GET, Method::DELETE] {
        let (status, _) = common::send(&app, method, &path, Some(&globex), None).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }
    let (status, _) = common::send(
        &app,
        Method::POST,
        &format!("{path}/pause"),
        Some(&globex),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[sqlx::test(migrator = "taut_db::MIGRATOR")]
async fn free_plan_quotas_are_enforced_per_kind(pool: PgPool) {
    let app = common::app(pool).await;
    let tenant = common::create_tenant(&app, "acme").await;
    for n in 0..10 {
        common::create_monitor(&app, &tenant, http_monitor(&format!("m{n}"))).await;
    }
    let (status, body) = common::send(
        &app,
        Method::POST,
        "/v1/monitors",
        Some(&tenant),
        Some(http_monitor("m10")),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert!(
        body["error"]["message"]
            .as_str()
            .unwrap()
            .contains("allows 10 monitors")
    );

    // Heartbeats have their own quota.
    common::create_monitor(
        &app,
        &tenant,
        json!({ "name": "cron", "type": "heartbeat" }),
    )
    .await;
}

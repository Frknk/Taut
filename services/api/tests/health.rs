//! Endpoint tests that run without a database.

mod common;

use http::StatusCode;
use tower::ServiceExt;

#[tokio::test]
async fn health_reports_ok_without_database() {
    let response = common::app_without_database()
        .oneshot(common::get("/health"))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = common::json(response).await;
    assert_eq!(body["status"], "ok");
    assert_eq!(body["service"], "taut-api");
    assert_eq!(body["version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(body["env"], "dev");
}

#[tokio::test]
async fn ready_reports_unavailable_when_database_is_down() {
    let response = common::app_without_database()
        .oneshot(common::get("/ready"))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    let body = common::json(response).await;
    assert_eq!(body["error"]["code"], "unavailable");
    assert!(body["error"]["trace_id"].is_string());
}

#[tokio::test]
async fn responses_carry_a_ulid_request_id() {
    let response = common::app_without_database()
        .oneshot(common::get("/health"))
        .await
        .unwrap();
    let id = response
        .headers()
        .get("x-request-id")
        .expect("header present")
        .to_str()
        .unwrap();
    assert_eq!(id.len(), 26, "a ULID is 26 characters: {id}");
}

#[tokio::test]
async fn incoming_request_id_is_preserved() {
    let request = http::Request::get("/health")
        .header("x-request-id", "gateway-123")
        .body(axum::body::Body::empty())
        .unwrap();
    let response = common::app_without_database()
        .oneshot(request)
        .await
        .unwrap();
    assert_eq!(
        response.headers().get("x-request-id").unwrap(),
        "gateway-123"
    );
}

#[tokio::test]
async fn unknown_route_returns_json_404() {
    let response = common::app_without_database()
        .oneshot(common::get("/nope"))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let body = common::json(response).await;
    assert_eq!(body["error"]["code"], "not_found");
    assert_eq!(body["error"]["trace_id"].as_str().map(str::len), Some(26));
}

#[tokio::test]
async fn ping_answers_pong() {
    let response = common::app_without_database()
        .oneshot(common::get("/v1/ping"))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(common::json(response).await["pong"], true);
}

#[tokio::test]
async fn metrics_expose_http_counters() {
    let app = common::app_without_database();
    app.clone().oneshot(common::get("/health")).await.unwrap();

    let response = app.oneshot(common::get("/metrics")).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let content_type = response
        .headers()
        .get("content-type")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(
        content_type.starts_with("text/plain"),
        "unexpected content type {content_type}"
    );
    let body = common::text(response).await;
    assert!(
        body.contains("http_requests_total"),
        "missing counter in:\n{body}"
    );
    assert!(
        body.contains("route=\"/health\""),
        "missing route label in:\n{body}"
    );
    assert!(
        body.contains("http_request_duration_seconds_bucket"),
        "missing histogram in:\n{body}"
    );
}

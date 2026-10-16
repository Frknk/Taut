//! Helpers shared by the integration tests.

#![allow(dead_code)] // each test binary uses a different subset

use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use http::{Method, Request, Response, StatusCode};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use taut_config::Settings;
use tower::ServiceExt;

/// Router whose pool points at a closed port: `/ready` fails fast, everything else works.
pub fn app_without_database() -> Router {
    let settings = Arc::new(Settings::default());
    let pool = PgPoolOptions::new()
        .acquire_timeout(Duration::from_millis(500))
        .connect_lazy("postgres://taut:taut@127.0.0.1:1/taut")
        .expect("a lazy pool never fails to build");
    app_with(settings, pool)
}

/// Router backed by the given pool.
pub fn app_with(settings: Arc<Settings>, pool: PgPool) -> Router {
    let telemetry = taut_telemetry::init(taut_api::SERVICE_NAME, &settings.telemetry)
        .expect("telemetry installs once per process");
    taut_api::app(settings, pool, telemetry.metrics().clone())
}

/// Router backed by `pool` with the default settings.
pub fn app(pool: PgPool) -> Router {
    app_with(Arc::new(Settings::default()), pool)
}

/// Sends a request with an optional JSON body and tenant header; returns status and JSON body
/// (`Value::Null` when the body is empty).
pub async fn send(
    app: &Router,
    method: Method,
    path: &str,
    tenant: Option<&str>,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut request = Request::builder().method(method).uri(path);
    if let Some(tenant) = tenant {
        request = request.header("x-tenant-id", tenant);
    }
    let request = match body {
        Some(body) => request
            .header("content-type", "application/json")
            .body(Body::from(body.to_string())),
        None => request.body(Body::empty()),
    }
    .expect("valid request");
    let response = app.clone().oneshot(request).await.expect("router answers");
    let status = response.status();
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("body is readable")
        .to_bytes();
    let body = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).expect("body is JSON")
    };
    (status, body)
}

/// Creates a tenant and returns its id.
pub async fn create_tenant(app: &Router, slug: &str) -> String {
    let (status, body) = send(
        app,
        Method::POST,
        "/v1/tenants",
        None,
        Some(json!({ "name": format!("Tenant {slug}"), "slug": slug })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    body["id"].as_str().expect("tenant id").to_owned()
}

/// Creates a monitor for `tenant` and returns the response body.
pub async fn create_monitor(app: &Router, tenant: &str, body: Value) -> Value {
    let (status, body) = send(app, Method::POST, "/v1/monitors", Some(tenant), Some(body)).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    body
}

/// Builds a `GET` request with an empty body.
pub fn get(path: &str) -> Request<Body> {
    Request::get(path)
        .body(Body::empty())
        .expect("valid request")
}

/// Reads a JSON response body.
pub async fn json(response: Response<Body>) -> serde_json::Value {
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("body is readable")
        .to_bytes();
    serde_json::from_slice(&bytes).expect("body is JSON")
}

/// Reads a text response body.
pub async fn text(response: Response<Body>) -> String {
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("body is readable")
        .to_bytes();
    String::from_utf8(bytes.to_vec()).expect("body is UTF-8")
}

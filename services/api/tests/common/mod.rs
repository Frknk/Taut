//! Helpers shared by the integration tests.

#![allow(dead_code)] // each test binary uses a different subset

use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use http::{Request, Response};
use http_body_util::BodyExt;
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use taut_config::Settings;

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

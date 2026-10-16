//! Prometheus scrape endpoint (`GET /metrics`).

// Axum handlers receive their state by value even when they only borrow it.
#![allow(clippy::needless_pass_by_value)]

use axum::Router;
use axum::extract::State;
use axum::http::header;
use axum::response::IntoResponse;
use axum::routing::get;
use metrics_exporter_prometheus::PrometheusHandle;

/// Prometheus text exposition format, version 0.0.4.
const CONTENT_TYPE: &str = "text/plain; version=0.0.4; charset=utf-8";

/// Router with `/metrics`.
pub fn router(handle: PrometheusHandle) -> Router {
    Router::new()
        .route("/metrics", get(render))
        .with_state(handle)
}

#[allow(clippy::unused_async)] // axum handlers must be async
async fn render(State(handle): State<PrometheusHandle>) -> impl IntoResponse {
    ([(header::CONTENT_TYPE, CONTENT_TYPE)], handle.render())
}

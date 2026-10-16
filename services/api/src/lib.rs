//! taut API: the control plane (tenants, monitors, alert channels).
//!
//! The router is built by [`app`] so integration tests can drive it in-process, without
//! opening a socket. `main.rs` only wires configuration, telemetry, the database and the
//! server loop around it.

use std::sync::Arc;

use axum::routing::get;
use axum::{Json, Router};
use metrics_exporter_prometheus::PrometheusHandle;
use serde::Serialize;
use sqlx::PgPool;
use taut_config::Settings;
use taut_http::{AppError, ServiceInfo, health, layers, metrics};

/// Name reported in logs, metrics and `/health`.
pub const SERVICE_NAME: &str = "taut-api";

/// State shared by the API handlers.
#[derive(Clone)]
pub struct AppState {
    /// Loaded configuration.
    pub settings: Arc<Settings>,
    /// PostgreSQL pool.
    pub pool: PgPool,
}

/// Builds the complete router: health, metrics, the `/v1` API and the middleware stack.
pub fn app(settings: Arc<Settings>, pool: PgPool, metrics_handle: PrometheusHandle) -> Router {
    let info = ServiceInfo::new(SERVICE_NAME, env!("CARGO_PKG_VERSION"), &settings.app);
    let http_settings = settings.http.clone();
    let state = AppState {
        settings,
        pool: pool.clone(),
    };

    let v1 = Router::new().route("/v1/ping", get(ping)).with_state(state);

    let router = Router::new()
        .merge(health::router(info, pool))
        .merge(metrics::router(metrics_handle))
        .merge(v1)
        .fallback(not_found);

    layers::apply(router, &http_settings)
}

#[derive(Serialize)]
struct Pong {
    pong: bool,
}

#[allow(clippy::unused_async)] // axum handlers must be async
async fn ping() -> Json<Pong> {
    Json(Pong { pong: true })
}

#[allow(clippy::unused_async)] // axum handlers must be async
async fn not_found() -> AppError {
    AppError::NotFound("route not found".to_owned())
}

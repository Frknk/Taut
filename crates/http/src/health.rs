//! Liveness (`GET /health`) and readiness (`GET /ready`) endpoints.
//!
//! Kubernetes restarts a pod when `/health` fails and stops routing traffic to it when
//! `/ready` fails, so liveness never depends on external systems while readiness does.

// Axum handlers receive their state by value even when they only borrow it.
#![allow(clippy::needless_pass_by_value)]

use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use serde::Serialize;
use sqlx::PgPool;
use taut_config::{AppSettings, Environment};

use crate::error::AppError;

/// Identity reported by the health endpoints.
#[derive(Debug, Clone, Serialize)]
pub struct ServiceInfo {
    /// Service name, for example `taut-api`. Serialized as `service`.
    #[serde(rename = "service")]
    pub name: &'static str,
    /// Crate version (`CARGO_PKG_VERSION`).
    pub version: &'static str,
    /// Git commit the binary was built from.
    pub commit: String,
    /// Deployment environment.
    pub env: Environment,
}

impl ServiceInfo {
    /// Builds the identity of a service from its name, version and app settings.
    pub fn new(name: &'static str, version: &'static str, app: &AppSettings) -> Self {
        Self {
            name,
            version,
            commit: app.commit.clone(),
            env: app.env,
        }
    }
}

#[derive(Clone)]
struct HealthState {
    info: ServiceInfo,
    pool: PgPool,
}

#[derive(Serialize)]
struct HealthResponse {
    status: &'static str,
    #[serde(flatten)]
    info: ServiceInfo,
}

#[derive(Serialize)]
struct ReadyResponse {
    status: &'static str,
    checks: Checks,
}

#[derive(Serialize)]
struct Checks {
    database: &'static str,
}

/// Router with `/health` and `/ready`.
pub fn router(info: ServiceInfo, pool: PgPool) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/ready", get(ready))
        .with_state(HealthState { info, pool })
}

#[allow(clippy::unused_async)] // axum handlers must be async
async fn health(State(state): State<HealthState>) -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        info: state.info,
    })
}

async fn ready(State(state): State<HealthState>) -> Result<Json<ReadyResponse>, AppError> {
    taut_db::ping(&state.pool).await.map_err(|err| {
        tracing::warn!(error = %err, "readiness check failed");
        AppError::Unavailable("database unavailable".to_owned())
    })?;
    Ok(Json(ReadyResponse {
        status: "ready",
        checks: Checks { database: "ok" },
    }))
}

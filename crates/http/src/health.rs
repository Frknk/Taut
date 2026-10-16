//! Liveness (`GET /health`) and readiness (`GET /ready`) endpoints.
//!
//! Kubernetes restarts a pod when `/health` fails and stops routing traffic to it when
//! `/ready` fails, so liveness never depends on external systems while readiness does. Each
//! service declares what readiness means for it with a list of [`Probe`]s: the API probes
//! the database, the checker probes NATS, the scheduler probes all three stores.

// Axum handlers receive their state by value even when they only borrow it.
#![allow(clippy::needless_pass_by_value)]

use std::collections::BTreeMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

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

type ProbeFuture = Pin<Box<dyn Future<Output = bool> + Send>>;

/// A dependency that `/ready` verifies: a name for the response and a check that answers
/// whether the dependency is usable right now.
#[derive(Clone)]
pub struct Probe {
    name: &'static str,
    check: Arc<dyn Fn() -> ProbeFuture + Send + Sync>,
}

impl Probe {
    /// Probe named `name` that is ready when `check` resolves to `true`.
    pub fn new<F, Fut>(name: &'static str, check: F) -> Self
    where
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: Future<Output = bool> + Send + 'static,
    {
        Self {
            name,
            check: Arc::new(move || Box::pin(check())),
        }
    }

    /// Probe that pings PostgreSQL.
    pub fn database(pool: PgPool) -> Self {
        Self::new("database", move || {
            let pool = pool.clone();
            async move { taut_db::ping(&pool).await.is_ok() }
        })
    }
}

#[derive(Clone)]
struct HealthState {
    info: ServiceInfo,
    probes: Arc<[Probe]>,
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
    checks: BTreeMap<&'static str, &'static str>,
}

/// Router with `/health` and `/ready`; `/ready` runs every probe.
pub fn router(info: ServiceInfo, probes: Vec<Probe>) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/ready", get(ready))
        .with_state(HealthState {
            info,
            probes: probes.into(),
        })
}

#[allow(clippy::unused_async)] // axum handlers must be async
async fn health(State(state): State<HealthState>) -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        info: state.info,
    })
}

async fn ready(State(state): State<HealthState>) -> Result<Json<ReadyResponse>, AppError> {
    let results = futures::future::join_all(state.probes.iter().map(|probe| (probe.check)())).await;
    let mut checks = BTreeMap::new();
    for (probe, ok) in state.probes.iter().zip(results) {
        if !ok {
            tracing::warn!(dependency = probe.name, "readiness check failed");
            return Err(AppError::Unavailable(format!("{} unavailable", probe.name)));
        }
        checks.insert(probe.name, "ok");
    }
    Ok(Json(ReadyResponse {
        status: "ready",
        checks,
    }))
}

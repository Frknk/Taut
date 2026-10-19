//! Taut API: the control plane (tenants, monitors, alert channels).
//!
//! The router is built by [`app`] so integration tests can drive it in-process, without
//! opening a socket. `main.rs` only wires configuration, telemetry, the database and the
//! server loop around it.

pub mod api_keys;
pub mod channels;
pub mod heartbeats;
pub mod incidents;
pub mod invitations;
pub mod members;
pub mod metrics_api;
pub mod monitors;
pub mod ratelimit;
pub mod tenant;
pub mod tenants;
pub mod validate;

use std::sync::Arc;

use axum::routing::get;
use axum::{Json, Router};
use metrics_exporter_prometheus::PrometheusHandle;
use serde::Serialize;
use sqlx::{PgPool, Postgres, Transaction};
use taut_bus::Bus;
use taut_config::Settings;
use taut_http::{AppError, Probe, ServiceInfo, health, layers, metrics};
use taut_identity::Verifier;
use uuid::Uuid;

use crate::ratelimit::TenantLimiter;

/// Name reported in logs, metrics and `/health`.
pub const SERVICE_NAME: &str = "taut-api";

/// State shared by the API handlers.
#[derive(Clone)]
pub struct AppState {
    /// Loaded configuration.
    pub settings: Arc<Settings>,
    /// PostgreSQL pool acting as the app role ([`taut_db::restricted`]): outside a
    /// [`tenant_tx`](Self::tenant_tx) its queries see no tenant data.
    pub pool: PgPool,
    /// NATS JetStream.
    pub bus: Bus,
    /// Verifies the access tokens the auth service signs.
    pub verifier: Arc<Verifier>,
    /// Per-tenant request budgets.
    pub limiter: TenantLimiter,
}

impl AppState {
    /// A transaction of `tenant_id`: row-level security lets through only its rows. Every
    /// handler that reads or writes tenant data runs in one.
    pub async fn tenant_tx(
        &self,
        tenant_id: Uuid,
    ) -> Result<Transaction<'static, Postgres>, sqlx::Error> {
        taut_db::begin_tenant(&self.pool, tenant_id).await
    }

    /// A transaction with the owner's rights, across tenants. Only for the lookups that find
    /// the tenant (API key, heartbeat token, invitation link) or list a user's tenants
    /// (ADR 0010).
    pub async fn system_tx(&self) -> Result<Transaction<'static, Postgres>, sqlx::Error> {
        taut_db::begin_system(&self.pool).await
    }
}

/// Builds the complete router: health, metrics, the `/v1` API, heartbeat pings and the
/// middleware stack. Readiness probes the database and that at least one signing key is
/// known (fetching the JWKS again if none is): without NATS the panel keeps working and only
/// heartbeat pings fail; without Redis requests go through uncounted. `owner` is the pool of
/// the tables' owner; the handlers get one restricted to the app role on the same database.
pub fn app(
    settings: Arc<Settings>,
    owner: &PgPool,
    bus: Bus,
    limiter: TenantLimiter,
    verifier: Arc<Verifier>,
    metrics_handle: PrometheusHandle,
) -> Router {
    let info = ServiceInfo::new(SERVICE_NAME, env!("CARGO_PKG_VERSION"), &settings.app);
    let http_settings = settings.http.clone();
    let jwks_probe = Probe::new("jwks", {
        let verifier = Arc::clone(&verifier);
        move || {
            let verifier = Arc::clone(&verifier);
            async move {
                if verifier.key_count() == 0 {
                    let _ = verifier.refresh().await;
                }
                verifier.key_count() > 0
            }
        }
    });
    let pool = taut_db::restricted(owner);
    let state = AppState {
        settings,
        pool: pool.clone(),
        bus,
        verifier,
        limiter,
    };

    let v1 = Router::new()
        .route("/v1/ping", get(ping))
        .merge(tenants::router())
        .merge(members::router())
        .merge(invitations::router())
        .merge(monitors::router())
        .merge(metrics_api::router())
        .merge(incidents::router())
        .merge(channels::router())
        .merge(api_keys::router())
        .merge(heartbeats::router())
        .with_state(state);

    let router = Router::new()
        .merge(health::router(
            info,
            vec![Probe::database(pool), jwks_probe],
        ))
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

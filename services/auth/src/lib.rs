//! taut auth service: who the user is.
//!
//! Registration, email verification, login, sessions (an access JWT plus a refresh cookie),
//! password recovery and the JWKS the other services verify tokens with. It owns the
//! `users` and `user_tokens` tables and the `session:*` and `rl:*` keys in Redis; tenants and
//! memberships belong to the api. Emails leave as events on the bus and the notifier sends
//! them.

pub mod current_user;
pub mod mail;
pub mod password;
pub mod ratelimit;
pub mod routes;
pub mod sessions;
pub mod tokens;
pub mod users;

use std::sync::Arc;

use axum::Router;
use metrics_exporter_prometheus::PrometheusHandle;
use redis::aio::ConnectionManager;
use sqlx::PgPool;
use taut_bus::Bus;
use taut_config::{Environment, Settings};
use taut_http::{AppError, Probe, ServiceInfo, health, layers, metrics};
use taut_identity::{KeyPair, Verifier};

/// Name reported in logs, metrics and `/health`.
pub const SERVICE_NAME: &str = "taut-auth";

/// State shared by the handlers.
#[derive(Clone)]
pub struct AppState {
    /// Loaded configuration.
    pub settings: Arc<Settings>,
    /// PostgreSQL pool.
    pub pool: PgPool,
    /// NATS JetStream, for the account emails.
    pub bus: Bus,
    /// Sessions and rate limits.
    pub sessions: sessions::SessionStore,
    /// Login and email limits.
    pub limiter: ratelimit::Limiter,
    /// The key that signs tokens.
    pub keys: Arc<KeyPair>,
    /// Verifies the tokens this service signed, for the routes that need a session.
    pub verifier: Arc<Verifier>,
    /// Whether cookies carry `Secure`; off in development over plain HTTP.
    pub secure_cookies: bool,
}

/// Builds the complete router: health, metrics, `/v1/auth/*`, the JWKS and the middleware.
pub fn app(
    settings: Arc<Settings>,
    pool: PgPool,
    bus: Bus,
    redis: ConnectionManager,
    keys: KeyPair,
    metrics_handle: PrometheusHandle,
) -> Router {
    let info = ServiceInfo::new(SERVICE_NAME, env!("CARGO_PKG_VERSION"), &settings.app);
    let http_settings = settings.http.clone();
    let verifier = Arc::new(Verifier::from_jwks(&keys.jwks(), &settings.auth.issuer));
    let secure_cookies = !matches!(settings.app.env, Environment::Dev | Environment::Test);
    let probes = vec![
        Probe::database(pool.clone()),
        Probe::new("redis", {
            let redis = redis.clone();
            move || {
                let mut redis = redis.clone();
                async move {
                    redis::cmd("PING")
                        .query_async::<String>(&mut redis)
                        .await
                        .is_ok()
                }
            }
        }),
        Probe::new("nats", {
            let bus = bus.clone();
            move || {
                let connected = bus.is_connected();
                async move { connected }
            }
        }),
    ];
    let state = AppState {
        sessions: sessions::SessionStore::new(redis.clone(), settings.auth.refresh_ttl_secs),
        limiter: ratelimit::Limiter::new(redis),
        settings,
        pool,
        bus,
        keys: Arc::new(keys),
        verifier,
        secure_cookies,
    };

    let router = Router::new()
        .merge(health::router(info, probes))
        .merge(metrics::router(metrics_handle))
        .merge(routes::router().with_state(state))
        .fallback(not_found);
    layers::apply(router, &http_settings)
}

#[allow(clippy::unused_async)] // axum handlers must be async
async fn not_found() -> AppError {
    AppError::NotFound("route not found".to_owned())
}

/// Maps a Redis failure to the generic internal error; the cause goes to the logs.
pub(crate) fn redis_error(err: &redis::RedisError) -> AppError {
    AppError::Internal(anyhow::anyhow!("redis: {err}"))
}

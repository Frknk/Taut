//! Taut auth service: who the user is.
//!
//! Registration, email verification, login (with an optional second factor: TOTP and backup
//! codes), sessions (an access JWT plus a refresh cookie), re-authentication for the
//! sensitive actions, password recovery and the JWKS the other services verify tokens with.
//! Registration, login and recovery also ask for a Cloudflare Turnstile token ([`captcha`]).
//! It owns the `users`, `user_tokens` and `mfa_backup_codes` tables and the `session:*`,
//! `rl:*`, `mfa:*` and `totp:*` keys in Redis; tenants and memberships belong to the api.
//! Emails leave as events on the bus and the notifier sends them.

pub mod captcha;
pub mod current_user;
pub mod mail;
pub mod mfa;
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
    /// Pending enrollments and replay marks of the second factor.
    pub mfa: mfa::MfaStore,
    /// Encrypts the TOTP secrets at rest.
    pub totp: mfa::TotpCipher,
    /// Turnstile check of the public forms.
    pub captcha: captcha::Captcha,
    /// The key that signs tokens.
    pub keys: Arc<KeyPair>,
    /// Verifies the tokens this service signed, for the routes that need a session.
    pub verifier: Arc<Verifier>,
    /// Whether cookies carry `Secure`; off in development over plain HTTP.
    pub secure_cookies: bool,
}

/// Builds the complete router: health, metrics, `/v1/auth/*`, the JWKS and the middleware.
/// `totp` encrypts the second-factor secrets (see [`totp_cipher`]) and `captcha` guards the
/// public forms (see [`captcha()`]).
#[allow(clippy::too_many_arguments)] // one per dependency, built by main and the tests
pub fn app(
    settings: Arc<Settings>,
    pool: PgPool,
    bus: Bus,
    redis: ConnectionManager,
    keys: KeyPair,
    totp: mfa::TotpCipher,
    captcha: captcha::Captcha,
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
        mfa: mfa::MfaStore::new(redis.clone()),
        limiter: ratelimit::Limiter::new(redis),
        totp,
        captcha,
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

/// The cipher for the TOTP secrets from `TAUT_AUTH__TOTP_KEY`; in development and tests a
/// fixed key when none is configured, elsewhere an error.
pub fn totp_cipher(settings: &Settings) -> anyhow::Result<mfa::TotpCipher> {
    if let Some(key) = settings.auth.totp_key()? {
        return Ok(mfa::TotpCipher::new(&key));
    }
    match settings.app.env {
        Environment::Dev | Environment::Test => {
            tracing::warn!("no TAUT_AUTH__TOTP_KEY configured: using the development key");
            Ok(mfa::TotpCipher::dev())
        }
        Environment::Staging | Environment::Prod => {
            anyhow::bail!("TAUT_AUTH__TOTP_KEY is required outside development")
        }
    }
}

/// The Turnstile check from `TAUT_AUTH__TURNSTILE_SECRET`; in development and tests, off when
/// none is configured, elsewhere an error.
pub fn captcha(settings: &Settings) -> anyhow::Result<captcha::Captcha> {
    let auth = &settings.auth;
    if !auth.turnstile_secret.trim().is_empty() {
        return captcha::Captcha::new(&auth.turnstile_secret, &auth.turnstile_url);
    }
    match settings.app.env {
        Environment::Dev | Environment::Test => {
            tracing::warn!("no TAUT_AUTH__TURNSTILE_SECRET configured: the captcha is off");
            Ok(captcha::Captcha::disabled())
        }
        Environment::Staging | Environment::Prod => {
            anyhow::bail!("TAUT_AUTH__TURNSTILE_SECRET is required outside development")
        }
    }
}

#[allow(clippy::unused_async)] // axum handlers must be async
async fn not_found() -> AppError {
    AppError::NotFound("route not found".to_owned())
}

/// Maps a Redis failure to the generic internal error; the cause goes to the logs.
pub(crate) fn redis_error(err: &redis::RedisError) -> AppError {
    AppError::Internal(anyhow::anyhow!("redis: {err}"))
}

//! Entry point of the Taut API service.

use std::sync::Arc;
use std::time::Duration;

use taut_api::ratelimit::TenantLimiter;
use taut_bus::Bus;
use taut_config::Settings;
use taut_identity::Verifier;

/// How often and how long the api waits for the auth service's JWKS at startup. It keeps
/// starting without it: the readiness probe fetches again, and so does the first token.
const JWKS_ATTEMPTS: u32 = 10;
const JWKS_RETRY: Duration = Duration::from_secs(2);

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let settings = Arc::new(Settings::load()?);
    let telemetry = taut_telemetry::init(taut_api::SERVICE_NAME, &settings.telemetry)?;
    tracing::info!(
        env = ?settings.app.env,
        version = env!("CARGO_PKG_VERSION"),
        commit = %settings.app.commit,
        "starting {}",
        taut_api::SERVICE_NAME
    );

    let pool = taut_db::connect(&settings.database).await?;
    if settings.database.migrate_on_start {
        taut_db::migrate(&pool).await?;
    }

    let bus = Bus::connect(&settings.nats, taut_api::SERVICE_NAME).await?;
    let limiter = TenantLimiter::lazy(&settings.redis.url)?;
    let verifier = Arc::new(load_verifier(&settings).await);

    let app = taut_api::app(
        Arc::clone(&settings),
        pool,
        bus,
        limiter,
        verifier,
        telemetry.metrics().clone(),
    );
    taut_http::serve(app, settings.http.addr).await?;
    Ok(())
}

/// Verifier pointed at the auth service, with its keys loaded when the service answers.
async fn load_verifier(settings: &Settings) -> Verifier {
    taut_net::install_crypto_provider();
    let url = format!(
        "{}/.well-known/jwks.json",
        settings.auth.url.trim_end_matches('/')
    );
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .expect("a default http client builds");
    let verifier = Verifier::remote(&url, &settings.auth.issuer, client);
    for attempt in 1..=JWKS_ATTEMPTS {
        match verifier.refresh().await {
            Ok(()) => return verifier,
            Err(err) => tracing::warn!(attempt, error = %err, "jwks not available yet"),
        }
        tokio::time::sleep(JWKS_RETRY).await;
    }
    tracing::error!(
        "starting without signing keys; every token is rejected until the auth service answers"
    );
    verifier
}

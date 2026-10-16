//! Entry point of the taut auth service.

use std::sync::Arc;

use taut_auth::SERVICE_NAME;
use taut_bus::Bus;
use taut_config::{Environment, Settings};
use taut_identity::KeyPair;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let settings = Arc::new(Settings::load()?);
    let telemetry = taut_telemetry::init(SERVICE_NAME, &settings.telemetry)?;
    tracing::info!(
        env = ?settings.app.env,
        version = env!("CARGO_PKG_VERSION"),
        commit = %settings.app.commit,
        "starting {SERVICE_NAME}"
    );

    let keys = load_keys(&settings)?;
    let pool = taut_db::connect(&settings.database).await?;
    if settings.database.migrate_on_start {
        taut_db::migrate(&pool).await?;
    }
    let bus = Bus::connect(&settings.nats, SERVICE_NAME).await?;
    let redis = redis::Client::open(settings.redis.url.as_str())?
        .get_connection_manager()
        .await?;

    let app = taut_auth::app(
        Arc::clone(&settings),
        pool,
        bus,
        redis,
        keys,
        telemetry.metrics().clone(),
    );
    taut_http::serve(app, settings.http.addr).await?;
    Ok(())
}

/// The signing key from the settings; in development, a fresh one when none is configured.
fn load_keys(settings: &Settings) -> anyhow::Result<KeyPair> {
    if let Some(pem) = settings.auth.signing_key_pem()? {
        let keys = KeyPair::from_pem(&pem)?;
        tracing::info!(kid = keys.kid(), "signing key loaded");
        return Ok(keys);
    }
    match settings.app.env {
        Environment::Dev | Environment::Test => {
            let keys = KeyPair::generate();
            tracing::warn!(
                kid = keys.kid(),
                "no signing key configured: using an ephemeral one, access tokens die with this process"
            );
            Ok(keys)
        }
        Environment::Staging | Environment::Prod => anyhow::bail!(
            "TAUT_AUTH__SIGNING_KEY or TAUT_AUTH__SIGNING_KEY_FILE is required outside development"
        ),
    }
}

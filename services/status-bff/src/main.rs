//! Entry point of the Taut status BFF.

use taut_config::Settings;
use taut_status_bff::SERVICE_NAME;
use taut_status_bff::cache::PageCache;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let settings = Settings::load()?;
    let telemetry = taut_telemetry::init(SERVICE_NAME, &settings.telemetry)?;
    tracing::info!(
        env = ?settings.app.env,
        version = env!("CARGO_PKG_VERSION"),
        commit = %settings.app.commit,
        cache_ttl_secs = settings.status.cache_ttl_secs,
        "starting {SERVICE_NAME}"
    );

    let pool = taut_db::connect(&settings.database).await?;
    let cache = PageCache::lazy(&settings.redis.url, settings.status.cache_ttl_secs)?;
    let app = taut_status_bff::app(&settings, pool, cache, telemetry.metrics().clone());
    taut_http::serve(app, settings.http.addr).await?;
    Ok(())
}

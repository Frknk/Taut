//! Entry point of the Taut incident service.

use std::sync::Arc;

use taut_bus::Bus;
use taut_config::Settings;
use taut_incidents::SERVICE_NAME;

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

    let pool = taut_db::connect(&settings.database).await?;
    let bus = Bus::connect(&settings.nats, SERVICE_NAME).await?;

    let app = taut_incidents::app(&settings, pool.clone(), &bus, telemetry.metrics().clone());
    let worker_settings = Arc::clone(&settings);
    taut_http::serve_with_worker(app, settings.http.addr, move |stop| {
        taut_incidents::run(worker_settings, pool, bus, stop)
    })
    .await?;
    Ok(())
}

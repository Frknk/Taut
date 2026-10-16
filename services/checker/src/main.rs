//! Entry point of the taut checker.

use std::sync::Arc;

use taut_bus::Bus;
use taut_checker::{Checker, SERVICE_NAME};
use taut_config::Settings;

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

    let bus = Bus::connect(&settings.nats, SERVICE_NAME).await?;
    let checker = Arc::new(Checker::new(settings.checks.allow_private_targets)?);

    let app = taut_checker::app(&settings, &bus, telemetry.metrics().clone());
    let worker_settings = Arc::clone(&settings);
    taut_http::serve_with_worker(app, settings.http.addr, move |stop| {
        taut_checker::run(worker_settings, bus, checker, stop)
    })
    .await?;
    Ok(())
}

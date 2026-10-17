//! Entry point of the Taut notifier.

use std::sync::Arc;

use taut_bus::Bus;
use taut_config::Settings;
use taut_notifier::send::Senders;
use taut_notifier::{Notifier, SERVICE_NAME};

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
    let senders = Senders::new(&settings.notifier, settings.checks.allow_private_targets)?;
    let notifier = Arc::new(Notifier::new(
        pool.clone(),
        senders,
        &settings.app.panel_url,
    ));

    let app = taut_notifier::app(&settings, pool, &bus, telemetry.metrics().clone());
    taut_http::serve_with_worker(app, settings.http.addr, move |stop| {
        taut_notifier::run(bus, notifier, stop)
    })
    .await?;
    Ok(())
}

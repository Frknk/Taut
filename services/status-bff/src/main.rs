//! Entry point of the Taut status BFF.

use taut_config::Settings;
use taut_status_bff::SERVICE_NAME;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let settings = Settings::load()?;
    let telemetry = taut_telemetry::init(SERVICE_NAME, &settings.telemetry)?;
    tracing::info!(
        env = ?settings.app.env,
        version = env!("CARGO_PKG_VERSION"),
        commit = %settings.app.commit,
        "starting {SERVICE_NAME}"
    );

    let pool = taut_db::connect(&settings.database).await?;
    let app = taut_status_bff::app(&settings, pool, telemetry.metrics().clone());
    taut_http::serve(app, settings.http.addr).await?;
    Ok(())
}

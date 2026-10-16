//! Entry point of the taut API service.

use std::sync::Arc;

use taut_config::Settings;

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

    let app = taut_api::app(Arc::clone(&settings), pool, telemetry.metrics().clone());
    taut_http::serve(app, settings.http.addr).await?;
    Ok(())
}

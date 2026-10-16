//! taut incident service: the consumer of `checks.completed`.
//!
//! Every result is stored in the `check_results` hypertable. The insert is keyed by
//! monitor, instant and region, so a redelivered event is a no-op; results of monitors that
//! no longer exist are skipped.

pub mod results;

use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use metrics_exporter_prometheus::PrometheusHandle;
use sqlx::PgPool;
use taut_bus::{Bus, CHECKS_STREAM, Delivery, Handled};
use taut_config::Settings;
use taut_http::{Probe, ServiceInfo, health, layers, metrics};
use taut_model::events::{CheckCompleted, subjects};
use tokio::sync::watch;

/// Name reported in logs, metrics and `/health`.
pub const SERVICE_NAME: &str = "taut-incidents";

/// Durable consumer shared by every replica.
const CONSUMER: &str = "incidents";
const ACK_WAIT: Duration = Duration::from_secs(30);
/// Results processed at the same time; each one is a short transaction.
const CONCURRENCY: usize = 32;

/// Router with `/health`, `/ready` (database and NATS) and `/metrics`.
pub fn app(
    settings: &Settings,
    pool: PgPool,
    bus: &Bus,
    metrics_handle: PrometheusHandle,
) -> Router {
    let info = ServiceInfo::new(SERVICE_NAME, env!("CARGO_PKG_VERSION"), &settings.app);
    let bus = bus.clone();
    let probes = vec![
        Probe::database(pool),
        Probe::new("nats", move || {
            let connected = bus.is_connected();
            async move { connected }
        }),
    ];
    let router = Router::new()
        .merge(health::router(info, probes))
        .merge(metrics::router(metrics_handle));
    layers::apply(router, &settings.http)
}

/// Consumes `checks.completed` until `stop` turns `true`.
pub async fn run(
    _settings: Arc<Settings>,
    pool: PgPool,
    bus: Bus,
    stop: watch::Receiver<bool>,
) -> anyhow::Result<()> {
    let consumer = bus
        .durable_consumer(
            CHECKS_STREAM,
            CONSUMER,
            subjects::CHECKS_COMPLETED,
            ACK_WAIT,
        )
        .await?;
    tracing::info!("incidents consuming check results");
    taut_bus::consume(
        consumer,
        CONCURRENCY,
        stop,
        |delivery: Delivery<CheckCompleted>| {
            let pool = pool.clone();
            async move { handle(&pool, &delivery.event).await }
        },
    )
    .await?;
    Ok(())
}

/// Records one result.
pub async fn handle(pool: &PgPool, result: &CheckCompleted) -> Handled {
    match results::record(pool, result).await {
        Ok(true) => {
            tracing::debug!(monitor_id = %result.monitor_id, ok = result.ok, "result recorded");
            ::metrics::counter!("check_results_recorded_total").increment(1);
            Handled::Done
        }
        Ok(false) => {
            tracing::debug!(monitor_id = %result.monitor_id, "duplicate or orphan result skipped");
            Handled::Done
        }
        Err(err) => Handled::Retry(err.to_string()),
    }
}

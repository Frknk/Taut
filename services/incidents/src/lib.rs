//! taut incident service: the consumer of `checks.completed`.
//!
//! Every result is stored in the `check_results` hypertable (keyed by monitor, instant and
//! region, so a redelivered event is a no-op) and applied to the monitor's state machine
//! ([`state`]). When an incident opens or resolves, the service publishes
//! `incidents.opened` / `incidents.resolved` for the notifier.

pub mod process;
pub mod results;
pub mod state;

use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use metrics_exporter_prometheus::PrometheusHandle;
use sqlx::PgPool;
use taut_bus::{Bus, CHECKS_STREAM, Delivery, Handled};
use taut_config::Settings;
use taut_http::{Probe, ServiceInfo, health, layers, metrics};
use taut_model::events::{CheckCompleted, IncidentChanged, subjects};
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
            let bus = bus.clone();
            async move { handle(&pool, &bus, &delivery.event).await }
        },
    )
    .await?;
    Ok(())
}

/// Publish attempts for an incident event before giving up.
const PUBLISH_ATTEMPTS: u32 = 3;

/// Applies one result and publishes the incident event it produced.
pub async fn handle(pool: &PgPool, bus: &Bus, result: &CheckCompleted) -> Handled {
    let applied = match process::apply_result(pool, result).await {
        Ok(applied) => applied,
        Err(err) => return Handled::Retry(err.to_string()),
    };
    if !applied.recorded {
        tracing::debug!(monitor_id = %result.monitor_id, "duplicate or orphan result skipped");
        return Handled::Done;
    }
    tracing::debug!(monitor_id = %result.monitor_id, ok = result.ok, "result applied");
    ::metrics::counter!("check_results_recorded_total").increment(1);

    if let Some((subject, event)) = applied.event {
        let transition = if event.resolved_at.is_none() {
            "opened"
        } else {
            "resolved"
        };
        tracing::info!(
            incident_id = %event.incident_id,
            monitor_id = %event.monitor_id,
            tenant_id = %event.tenant_id,
            cause = %event.cause,
            transition,
            "incident {transition}"
        );
        ::metrics::counter!("incidents_total", "transition" => transition).increment(1);
        publish(bus, subject, &event).await;
    }
    Handled::Done
}

/// Publishes an incident event, retrying a few times. The transition is already committed,
/// so a redelivered result would not produce the event again: if every attempt fails the
/// alert is lost, which is logged as an error (a transactional outbox fixes it, weeks 10–11).
async fn publish(bus: &Bus, subject: &str, event: &IncidentChanged) {
    // The stream drops duplicates of this id, so a retry after a lost ack is harmless.
    let message_id = format!("{}.{subject}", event.incident_id);
    for attempt in 1..=PUBLISH_ATTEMPTS {
        match bus
            .publish_once(subject, &event.trace_id, &message_id, event)
            .await
        {
            Ok(()) => return,
            Err(err) if attempt < PUBLISH_ATTEMPTS => {
                tracing::warn!(error = %err, attempt, "incident event not published, retrying");
                tokio::time::sleep(Duration::from_secs(u64::from(attempt))).await;
            }
            Err(err) => {
                tracing::error!(error = %err, incident_id = %event.incident_id, "incident event lost");
                ::metrics::counter!("incident_events_lost_total").increment(1);
            }
        }
    }
}

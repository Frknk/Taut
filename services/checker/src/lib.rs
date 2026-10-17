//! Taut checker: runs the checks the scheduler requests and publishes their results.
//!
//! Workers share the durable consumer `checker` on `checks.requested`, so adding replicas
//! (or regions, in prototype 2) spreads the work. A worker never touches the database:
//! everything it needs travels in the request, and the result goes back to the bus as
//! `checks.completed`.

pub mod checks;

use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use chrono::Utc;
use metrics_exporter_prometheus::PrometheusHandle;
use taut_bus::{Bus, CHECKS_STREAM, Delivery, Handled};
use taut_config::Settings;
use taut_http::{Probe, ServiceInfo, health, layers, metrics};
use taut_model::events::{CheckCompleted, CheckRequested, subjects};
use tokio::sync::watch;
use tracing::Instrument;

pub use checks::{Checker, Outcome};

/// Name reported in logs, metrics and `/health`.
pub const SERVICE_NAME: &str = "taut-checker";

/// Durable consumer shared by every checker.
const CONSUMER: &str = "checker";
/// Longer than the longest timeout (60 s) so a slow check is not redelivered mid-flight.
const ACK_WAIT: Duration = Duration::from_secs(90);

/// Router with `/health`, `/ready` (NATS connected) and `/metrics`.
pub fn app(settings: &Settings, bus: &Bus, metrics_handle: PrometheusHandle) -> Router {
    let info = ServiceInfo::new(SERVICE_NAME, env!("CARGO_PKG_VERSION"), &settings.app);
    let bus = bus.clone();
    let nats = Probe::new("nats", move || {
        let connected = bus.is_connected();
        async move { connected }
    });
    let router = Router::new()
        .merge(health::router(info, vec![nats]))
        .merge(metrics::router(metrics_handle));
    layers::apply(router, &settings.http)
}

/// Consumes `checks.requested` until `stop` turns `true`.
pub async fn run(
    settings: Arc<Settings>,
    bus: Bus,
    checker: Arc<Checker>,
    stop: watch::Receiver<bool>,
) -> anyhow::Result<()> {
    let consumer = bus
        .durable_consumer(
            CHECKS_STREAM,
            CONSUMER,
            subjects::CHECKS_REQUESTED,
            ACK_WAIT,
        )
        .await?;
    tracing::info!(concurrency = settings.checks.concurrency, region = %settings.checks.region, "checker consuming");
    let region: Arc<str> = settings.checks.region.as_str().into();
    taut_bus::consume(
        consumer,
        settings.checks.concurrency,
        stop,
        |delivery: Delivery<CheckRequested>| {
            let bus = bus.clone();
            let checker = Arc::clone(&checker);
            let region = Arc::clone(&region);
            async move { handle(&bus, &checker, &region, delivery.event).await }
        },
    )
    .await?;
    Ok(())
}

/// Runs one requested check and publishes its result.
pub async fn handle(
    bus: &Bus,
    checker: &Checker,
    region: &str,
    request: CheckRequested,
) -> Handled {
    let span = tracing::info_span!(
        "check",
        monitor_id = %request.monitor_id,
        tenant_id = %request.tenant_id,
        kind = %request.kind,
    );
    async {
        let Some(completed) = execute(checker, region, &request).await else {
            return Handled::Done;
        };
        match bus
            .publish(subjects::CHECKS_COMPLETED, &request.trace_id, &completed)
            .await
        {
            Ok(()) => Handled::Done,
            Err(err) => Handled::Retry(err.to_string()),
        }
    }
    .instrument(span)
    .await
}

/// Runs `request` unless it expired; returns the result to publish.
pub async fn execute(
    checker: &Checker,
    region: &str,
    request: &CheckRequested,
) -> Option<CheckCompleted> {
    let kind = request.kind.as_str();
    let started = Utc::now();
    if started > request.expires_at {
        tracing::debug!("check expired before it could run, skipping");
        ::metrics::counter!("uptime_checks_expired_total", "type" => kind).increment(1);
        return None;
    }
    let lag = (started - request.scheduled_at)
        .to_std()
        .unwrap_or_default();
    ::metrics::histogram!("uptime_check_lag_seconds").record(lag.as_secs_f64());

    let outcome = checker.run(request).await;
    let result = if outcome.ok { "up" } else { "down" };
    ::metrics::counter!("uptime_checks_total", "type" => kind, "result" => result, "region" => region.to_owned())
        .increment(1);
    ::metrics::histogram!("uptime_check_latency_seconds", "type" => kind, "region" => region.to_owned())
        .record(f64::from(outcome.latency_ms) / 1000.0);
    tracing::info!(
        ok = outcome.ok,
        latency_ms = outcome.latency_ms,
        status = outcome.status,
        error = outcome.error.as_deref(),
        "check finished"
    );

    Some(CheckCompleted {
        trace_id: request.trace_id.clone(),
        monitor_id: request.monitor_id,
        tenant_id: request.tenant_id,
        at: started,
        region: region.to_owned(),
        ok: outcome.ok,
        status: outcome.status,
        latency_ms: Some(outcome.latency_ms),
        error: outcome.error,
    })
}

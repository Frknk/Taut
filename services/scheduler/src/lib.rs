//! taut scheduler: asks for every active monitor's check at its interval.
//!
//! The leader (see [`leader`]) loads the active monitors, keeps a [`schedule::Schedule`] and
//! every second publishes `checks.requested` for the monitors that are due. It reacts to
//! `monitors.*` events from the API at once and reloads everything from the database every
//! `scheduler.resync_secs` to repair missed events. Heartbeats are not sent to the checkers:
//! when one is due, the scheduler looks at its last ping and, if it is late, publishes the
//! failed `checks.completed` itself.

pub mod leader;
pub mod schedule;
pub mod store;

use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use chrono::Utc;
use futures::StreamExt;
use metrics_exporter_prometheus::PrometheusHandle;
use redis::aio::ConnectionManager;
use sqlx::PgPool;
use taut_bus::Bus;
use taut_config::Settings;
use taut_http::{Probe, ServiceInfo, health, layers, metrics};
use taut_model::MonitorKind;
use taut_model::events::{CheckCompleted, CheckRequested, MonitorChanged, subjects};
use tokio::sync::watch;
use tokio::time::{self, MissedTickBehavior};

use crate::leader::{LOCK_KEY, LeaderLock};
use crate::schedule::{Due, Schedule};
use crate::store::OverdueHeartbeat;

/// Name reported in logs, metrics and `/health`.
pub const SERVICE_NAME: &str = "taut-scheduler";

/// How often due monitors are collected.
const TICK: Duration = Duration::from_secs(1);
/// How long a follower waits before trying to take the lock again.
const FOLLOWER_RETRY: Duration = Duration::from_secs(2);

/// Router with `/health`, `/ready` (database, NATS and Redis) and `/metrics`.
pub fn app(
    settings: &Settings,
    pool: PgPool,
    bus: &Bus,
    redis: ConnectionManager,
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
        Probe::new("redis", move || {
            let mut redis = redis.clone();
            async move {
                redis::cmd("PING")
                    .query_async::<String>(&mut redis)
                    .await
                    .is_ok()
            }
        }),
    ];
    let router = Router::new()
        .merge(health::router(info, probes))
        .merge(metrics::router(metrics_handle));
    layers::apply(router, &settings.http)
}

/// Competes for leadership and schedules while leading, until `stop` turns `true`.
pub async fn run(
    settings: Arc<Settings>,
    pool: PgPool,
    bus: Bus,
    redis: ConnectionManager,
    mut stop: watch::Receiver<bool>,
) -> anyhow::Result<()> {
    let ttl = Duration::from_secs(settings.scheduler.lock_ttl_secs);
    let mut lock = LeaderLock::new(redis, LOCK_KEY, ttl);
    let mut events = bus.client().subscribe(subjects::MONITORS_ALL).await?;
    let leader = Leader {
        settings: &settings,
        pool: &pool,
        bus: &bus,
    };

    while !stopping(&stop) {
        match lock.acquire().await {
            Ok(true) => {
                tracing::info!("leading the schedule");
                ::metrics::gauge!("scheduler_leader").set(1.0);
                let ended = leader.lead(&mut lock, &mut events, &mut stop).await;
                ::metrics::gauge!("scheduler_leader").set(0.0);
                match ended {
                    Ok(Ended::Stopped) => {
                        if let Err(err) = lock.release().await {
                            tracing::warn!(error = %err, "could not release the leader lock");
                        }
                        break;
                    }
                    Ok(Ended::LostLock) => tracing::warn!("lost the leader lock"),
                    Err(err) => {
                        tracing::error!(error = %err, "leader failed, stepping down");
                        let _ = lock.release().await;
                    }
                }
            }
            Ok(false) => tracing::debug!("another replica leads"),
            Err(err) => tracing::warn!(error = %err, "redis unavailable, cannot take the lock"),
        }
        follow(&mut events, &mut stop).await;
    }
    Ok(())
}

fn stopping(stop: &watch::Receiver<bool>) -> bool {
    *stop.borrow() || stop.has_changed().is_err()
}

/// Waits before the next attempt to lead, discarding monitor events meanwhile: the new
/// leader reloads everything from the database anyway.
async fn follow(events: &mut async_nats::Subscriber, stop: &mut watch::Receiver<bool>) {
    let wait = time::sleep(FOLLOWER_RETRY);
    tokio::pin!(wait);
    loop {
        tokio::select! {
            () = &mut wait => return,
            _ = stop.changed() => return,
            message = events.next() => if message.is_none() { return },
        }
    }
}

enum Ended {
    Stopped,
    LostLock,
}

struct Leader<'a> {
    settings: &'a Settings,
    pool: &'a PgPool,
    bus: &'a Bus,
}

impl Leader<'_> {
    async fn lead(
        &self,
        lock: &mut LeaderLock,
        events: &mut async_nats::Subscriber,
        stop: &mut watch::Receiver<bool>,
    ) -> anyhow::Result<Ended> {
        let mut schedule = Schedule::default();
        schedule.sync(store::active_monitors(self.pool).await?, Utc::now());
        record_size(&schedule);

        let mut tick = time::interval(TICK);
        tick.set_missed_tick_behavior(MissedTickBehavior::Skip);
        let mut resync = time::interval(Duration::from_secs(self.settings.scheduler.resync_secs));
        resync.reset();
        let mut renew =
            time::interval(Duration::from_secs(self.settings.scheduler.lock_ttl_secs) / 3);
        renew.reset();

        loop {
            tokio::select! {
                _ = tick.tick() => {
                    let due = schedule.take_due(Utc::now());
                    if !due.is_empty() {
                        dispatch(self.pool, self.bus, &self.settings.checks.region, due).await;
                    }
                }
                _ = resync.tick() => match store::active_monitors(self.pool).await {
                    Ok(monitors) => {
                        schedule.sync(monitors, Utc::now());
                        record_size(&schedule);
                    }
                    Err(err) => tracing::warn!(error = %err, "resync failed, keeping the schedule"),
                },
                _ = renew.tick() => {
                    if !lock.renew().await? {
                        return Ok(Ended::LostLock);
                    }
                }
                message = events.next() => {
                    let Some(message) = message else {
                        anyhow::bail!("monitor event subscription closed");
                    };
                    self.apply_event(&mut schedule, &message.payload).await;
                    record_size(&schedule);
                }
                _ = stop.changed() => return Ok(Ended::Stopped),
            }
        }
    }

    async fn apply_event(&self, schedule: &mut Schedule, payload: &[u8]) {
        let event: MonitorChanged = match serde_json::from_slice(payload) {
            Ok(event) => event,
            Err(err) => {
                tracing::warn!(error = %err, "ignoring undecodable monitor event");
                return;
            }
        };
        match store::active_monitor(self.pool, event.monitor_id).await {
            Ok(Some(monitor)) => schedule.upsert(monitor, Utc::now()),
            Ok(None) => {
                schedule.remove(event.monitor_id);
            }
            Err(err) => {
                tracing::warn!(
                    error = %err,
                    monitor_id = %event.monitor_id,
                    "cannot load monitor; the next resync will"
                );
            }
        }
    }
}

fn record_size(schedule: &Schedule) {
    #[allow(clippy::cast_precision_loss)] // a gauge; thousands of monitors fit exactly
    ::metrics::gauge!("scheduler_monitors").set(schedule.len() as f64);
}

/// Publishes the checks of the due monitors and the failures of late heartbeats. Errors are
/// logged, not returned: one failed publish must not stop the rest of the schedule.
pub async fn dispatch(pool: &PgPool, bus: &Bus, region: &str, due: Vec<Due>) {
    let mut heartbeats = Vec::new();
    for item in due {
        if item.monitor.kind == MonitorKind::Heartbeat {
            heartbeats.push(item.monitor.id);
            continue;
        }
        let Some(request) = check_request(&item) else {
            tracing::warn!(monitor_id = %item.monitor.id, "monitor without target, skipping");
            continue;
        };
        match bus
            .publish(subjects::CHECKS_REQUESTED, &request.trace_id, &request)
            .await
        {
            Ok(()) => {
                // The trace id is born here; this line starts the trail scheduler → checker →
                // incidents in the logs.
                tracing::debug!(
                    trace_id = %request.trace_id,
                    monitor_id = %request.monitor_id,
                    tenant_id = %request.tenant_id,
                    "check requested"
                );
                ::metrics::counter!("uptime_checks_scheduled_total", "type" => request.kind.as_str())
                    .increment(1);
            }
            Err(err) => {
                tracing::warn!(error = %err, monitor_id = %request.monitor_id, "cannot request check");
            }
        }
    }
    if heartbeats.is_empty() {
        return;
    }
    match store::overdue_heartbeats(pool, &heartbeats).await {
        Ok(overdue) => {
            for heartbeat in overdue {
                let missed = missed_heartbeat(&heartbeat, region);
                if let Err(err) = bus
                    .publish(subjects::CHECKS_COMPLETED, &missed.trace_id, &missed)
                    .await
                {
                    tracing::warn!(error = %err, monitor_id = %heartbeat.id, "cannot report missed heartbeat");
                } else {
                    ::metrics::counter!("uptime_heartbeats_missed_total").increment(1);
                }
            }
        }
        Err(err) => tracing::warn!(error = %err, "cannot read heartbeats"),
    }
}

/// The `checks.requested` event for a due monitor, with a new trace id. It expires when the
/// next check is due.
pub fn check_request(due: &Due) -> Option<CheckRequested> {
    let monitor = &due.monitor;
    let interval = chrono::Duration::seconds(monitor.interval_s.into());
    Some(CheckRequested {
        trace_id: ulid::Ulid::generate().to_string(),
        monitor_id: monitor.id,
        tenant_id: monitor.tenant_id,
        kind: monitor.kind,
        target: monitor.target.clone()?,
        timeout_s: u32::try_from(monitor.timeout_s).ok()?,
        expected_status: monitor
            .expected_status
            .and_then(|status| u16::try_from(status).ok()),
        keyword: monitor.keyword.clone(),
        scheduled_at: due.scheduled_at,
        expires_at: due.scheduled_at + interval,
    })
}

/// The failed result published for a heartbeat that missed its deadline.
pub fn missed_heartbeat(heartbeat: &OverdueHeartbeat, region: &str) -> CheckCompleted {
    let now = Utc::now();
    let silent = (now - heartbeat.last_ping_at).num_seconds();
    CheckCompleted {
        trace_id: ulid::Ulid::generate().to_string(),
        monitor_id: heartbeat.id,
        tenant_id: heartbeat.tenant_id,
        at: now,
        region: region.to_owned(),
        ok: false,
        status: None,
        latency_ms: None,
        error: Some(format!(
            "no ping for {silent} s (expected every {} s + {} s grace)",
            heartbeat.interval_s, heartbeat.grace_period_s
        )),
    }
}

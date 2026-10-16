//! taut notifier: turns incident events into alerts.
//!
//! It consumes `incidents.opened` and `incidents.resolved` and sends one message per active
//! channel of the tenant (F-40 email, F-41 webhooks for Discord, Slack or any endpoint).
//! Every delivery is recorded in `notification_deliveries`: a redelivered event skips the
//! channels that already got it and retries only the ones that failed. It also consumes
//! `notifications.test`, the "send test" button of the panel, and the account emails of the
//! `ACCOUNTS` stream (`users.>`, `tenants.>`): verification links, password resets,
//! invitations.

pub mod channels;
pub mod render;
pub mod send;

use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use metrics_exporter_prometheus::PrometheusHandle;
use sqlx::PgPool;
use taut_bus::{ACCOUNTS_STREAM, Bus, Delivery, Handled, INCIDENTS_STREAM};
use taut_config::Settings;
use taut_http::{Probe, ServiceInfo, health, layers, metrics};
use taut_model::events::{AccountEmail, IncidentChanged, TestNotification, subjects};
use tokio::sync::watch;

use crate::channels::Channel;
use crate::render::Rendered;
use crate::send::Senders;

/// Name reported in logs, metrics and `/health`.
pub const SERVICE_NAME: &str = "taut-notifier";

/// Several channels with a 10 s timeout each fit comfortably.
const ACK_WAIT: Duration = Duration::from_secs(60);
const CONCURRENCY: usize = 8;

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

/// Sends alerts.
pub struct Notifier {
    pool: PgPool,
    senders: Senders,
    panel_url: String,
}

impl Notifier {
    /// Notifier over `pool` with the given senders; links point at `panel_url`.
    pub fn new(pool: PgPool, senders: Senders, panel_url: &str) -> Self {
        Self {
            pool,
            senders,
            panel_url: panel_url.to_owned(),
        }
    }

    /// Sends an incident transition to every active channel of its tenant that did not get
    /// it yet. Retries the event while any channel fails.
    pub async fn incident(&self, event: &IncidentChanged) -> Handled {
        let transition = if event.resolved_at.is_none() {
            "opened"
        } else {
            "resolved"
        };
        let channels = match channels::active(&self.pool, event.tenant_id).await {
            Ok(channels) => channels,
            Err(err) => return Handled::Retry(err.to_string()),
        };
        if channels.is_empty() {
            tracing::debug!(incident_id = %event.incident_id, "tenant has no alert channels");
            return Handled::Done;
        }

        let message = render::incident(event, &self.panel_url);
        let mut failed = 0;
        for channel in &channels {
            match channels::already_sent(&self.pool, event.incident_id, channel.id, transition)
                .await
            {
                Ok(true) => continue,
                Ok(false) => {}
                Err(err) => return Handled::Retry(err.to_string()),
            }
            let result = self.deliver(channel, &message).await;
            if let Err(err) = channels::record_delivery(
                &self.pool,
                event.incident_id,
                channel,
                transition,
                result.as_ref().err().map(String::as_str),
            )
            .await
            {
                tracing::warn!(error = %err, channel_id = %channel.id, "delivery not recorded");
            }
            if result.is_err() {
                failed += 1;
            }
        }
        if failed > 0 {
            Handled::Retry(format!("{failed} of {} channels failed", channels.len()))
        } else {
            Handled::Done
        }
    }

    /// Sends the test message to one channel. Not retried: the panel shows the outcome.
    pub async fn test(&self, request: &TestNotification) -> Handled {
        let channel = match channels::find(&self.pool, request.tenant_id, request.channel_id).await
        {
            Ok(Some(channel)) => channel,
            Ok(None) => return Handled::Drop("channel no longer exists".to_owned()),
            Err(err) => return Handled::Retry(err.to_string()),
        };
        let result = self
            .deliver(&channel, &render::test_message(&channel.name))
            .await;
        if let Err(err) = channels::record_last_delivery(
            &self.pool,
            channel.id,
            result.as_ref().err().map(String::as_str),
        )
        .await
        {
            tracing::warn!(error = %err, channel_id = %channel.id, "test outcome not recorded");
        }
        Handled::Done
    }

    /// Sends an account email. SMTP failures are retried: the user is waiting for the link.
    pub async fn account_email(&self, email: &AccountEmail) -> Handled {
        let rendered = render::account_email(email);
        let result = self.senders.email(email.to(), &rendered).await;
        let outcome = if result.is_ok() { "sent" } else { "failed" };
        ::metrics::counter!("account_emails_total", "subject" => email.subject(), "result" => outcome)
            .increment(1);
        match result {
            Ok(()) => {
                tracing::info!(subject = email.subject(), "account email sent");
                Handled::Done
            }
            Err(error) => {
                tracing::warn!(subject = email.subject(), error = %error, "account email failed");
                Handled::Retry(error)
            }
        }
    }

    async fn deliver(&self, channel: &Channel, message: &Rendered) -> Result<(), String> {
        let result = self.senders.send(channel, message).await;
        let outcome = if result.is_ok() { "sent" } else { "failed" };
        ::metrics::counter!("notifications_total", "kind" => channel.kind.as_str(), "result" => outcome)
            .increment(1);
        match &result {
            Ok(()) => tracing::info!(
                channel_id = %channel.id,
                tenant_id = %channel.tenant_id,
                kind = %channel.kind,
                "notification sent"
            ),
            Err(error) => tracing::warn!(
                channel_id = %channel.id,
                tenant_id = %channel.tenant_id,
                kind = %channel.kind,
                error = %error,
                "notification failed"
            ),
        }
        result
    }
}

/// Consumes incident events and test requests until `stop` turns `true`.
pub async fn run(
    bus: Bus,
    notifier: Arc<Notifier>,
    stop: watch::Receiver<bool>,
) -> anyhow::Result<()> {
    let incidents = bus
        .durable_consumer(
            INCIDENTS_STREAM,
            "notifier",
            subjects::INCIDENTS_ALL,
            ACK_WAIT,
        )
        .await?;
    let tests = bus
        .durable_consumer(
            INCIDENTS_STREAM,
            "notifier-tests",
            subjects::NOTIFICATIONS_TEST,
            ACK_WAIT,
        )
        .await?;
    let users = bus
        .durable_consumer(
            ACCOUNTS_STREAM,
            "notifier-users",
            subjects::USERS_ALL,
            ACK_WAIT,
        )
        .await?;
    let tenants = bus
        .durable_consumer(
            ACCOUNTS_STREAM,
            "notifier-tenants",
            subjects::TENANTS_ALL,
            ACK_WAIT,
        )
        .await?;
    tracing::info!("notifier consuming incidents, test requests and account emails");
    let alerts = taut_bus::consume(
        incidents,
        CONCURRENCY,
        stop.clone(),
        |delivery: Delivery<IncidentChanged>| {
            let notifier = Arc::clone(&notifier);
            async move { notifier.incident(&delivery.event).await }
        },
    );
    let tests = taut_bus::consume(
        tests,
        CONCURRENCY,
        stop.clone(),
        |delivery: Delivery<TestNotification>| {
            let notifier = Arc::clone(&notifier);
            async move { notifier.test(&delivery.event).await }
        },
    );
    let users = taut_bus::consume(
        users,
        CONCURRENCY,
        stop.clone(),
        |delivery: Delivery<AccountEmail>| {
            let notifier = Arc::clone(&notifier);
            async move { notifier.account_email(&delivery.event).await }
        },
    );
    let tenants = taut_bus::consume(
        tenants,
        CONCURRENCY,
        stop,
        |delivery: Delivery<AccountEmail>| {
            let notifier = Arc::clone(&notifier);
            async move { notifier.account_email(&delivery.event).await }
        },
    );
    tokio::try_join!(alerts, tests, users, tenants)?;
    Ok(())
}

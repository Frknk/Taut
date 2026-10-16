//! NATS JetStream for taut services.
//!
//! [`Bus::connect`] opens the connection and makes sure the three streams exist: `CHECKS`
//! (`checks.>`), `INCIDENTS` (`incidents.>` and `notifications.>`) and `MONITORS`
//! (`monitors.>`). Events are JSON;
//! [`Bus::publish`] waits for the stream's acknowledgement and copies the event's trace id into
//! the `x-request-id` header. [`consume`] runs a durable pull consumer: it decodes each message,
//! opens a span with the trace id, calls the handler with bounded concurrency and acks, naks or
//! terminates the message according to the handler's answer.
//!
//! Delivery is at least once, so handlers must be idempotent.

use std::future::Future;
use std::time::Duration;

use async_nats::connection::State;
use async_nats::jetstream::consumer::pull::Config as PullConfig;
use async_nats::jetstream::consumer::{AckPolicy, PullConsumer};
use async_nats::jetstream::message::AckKind;
use async_nats::jetstream::stream::{Config as StreamConfig, RetentionPolicy, StorageType};
use async_nats::jetstream::{self, Context};
use async_nats::{ConnectOptions, HeaderMap};
use futures::StreamExt;
use serde::Serialize;
use serde::de::DeserializeOwned;
use taut_config::NatsSettings;
use tokio::sync::watch;
use tracing::Instrument;

/// Header that carries the trace id, the same one HTTP requests use.
pub const TRACE_ID_HEADER: &str = "x-request-id";

/// Stream of check requests and results.
pub const CHECKS_STREAM: &str = "CHECKS";
/// Stream of incident transitions.
pub const INCIDENTS_STREAM: &str = "INCIDENTS";
/// Stream of monitor changes.
pub const MONITORS_STREAM: &str = "MONITORS";

/// Name, subjects and retention of every stream. Messages are kept for a while even after
/// they are consumed, so a projection can be rebuilt by replaying them (week 10, CQRS).
const STREAMS: [(&str, &[&str], Duration); 3] = [
    (CHECKS_STREAM, &["checks.>"], Duration::from_hours(24)),
    (
        INCIDENTS_STREAM,
        &["incidents.>", "notifications.>"],
        Duration::from_hours(7 * 24),
    ),
    (MONITORS_STREAM, &["monitors.>"], Duration::from_hours(24)),
];

/// Bus failure.
#[derive(Debug, thiserror::Error)]
pub enum BusError {
    /// The connection could not be opened.
    #[error("nats connection failed: {0}")]
    Connect(#[from] async_nats::ConnectError),
    /// A stream could not be created or read.
    #[error("jetstream stream setup failed: {0}")]
    Stream(String),
    /// A consumer could not be created or started.
    #[error("jetstream consumer failed: {0}")]
    Consumer(String),
    /// The event could not be serialized.
    #[error("event serialization failed: {0}")]
    Encode(#[from] serde_json::Error),
    /// The stream did not acknowledge the message.
    #[error("publish failed: {0}")]
    Publish(String),
}

/// Connection to NATS and its JetStream context. Cloning is cheap.
#[derive(Clone)]
pub struct Bus {
    client: async_nats::Client,
    jetstream: Context,
}

impl Bus {
    /// Connects to NATS and creates the streams that do not exist yet.
    pub async fn connect(settings: &NatsSettings, name: &str) -> Result<Self, BusError> {
        let client = ConnectOptions::new()
            .name(name)
            .connect(settings.url.as_str())
            .await?;
        let bus = Self::from_client(client);
        bus.ensure_streams().await?;
        tracing::info!(url = %settings.url, "nats connected");
        Ok(bus)
    }

    /// Bus that connects in the background and does not create streams. For tests and tools
    /// that must start even when NATS is down; publishing fails until it connects.
    pub async fn lazy(url: &str) -> Result<Self, BusError> {
        let client = ConnectOptions::new()
            .retry_on_initial_connect()
            .connect(url)
            .await?;
        Ok(Self::from_client(client))
    }

    fn from_client(client: async_nats::Client) -> Self {
        let jetstream = jetstream::new(client.clone());
        Self { client, jetstream }
    }

    /// Creates the streams, or updates their subjects and retention when they changed.
    async fn ensure_streams(&self) -> Result<(), BusError> {
        for (name, subjects, max_age) in STREAMS {
            self.jetstream
                .create_or_update_stream(StreamConfig {
                    name: name.to_owned(),
                    subjects: subjects
                        .iter()
                        .map(|subject| (*subject).to_owned())
                        .collect(),
                    retention: RetentionPolicy::Limits,
                    storage: StorageType::File,
                    max_age,
                    duplicate_window: Duration::from_secs(120),
                    ..StreamConfig::default()
                })
                .await
                .map_err(|err| BusError::Stream(format!("{name}: {err}")))?;
        }
        Ok(())
    }

    /// Whether the connection to NATS is up; used by `/ready`.
    pub fn is_connected(&self) -> bool {
        matches!(self.client.connection_state(), State::Connected)
    }

    /// The underlying NATS client, for plain (non-JetStream) subscriptions.
    pub fn client(&self) -> &async_nats::Client {
        &self.client
    }

    /// Publishes `event` on `subject` and waits for the stream to store it.
    pub async fn publish<T: Serialize>(
        &self,
        subject: &str,
        trace_id: &str,
        event: &T,
    ) -> Result<(), BusError> {
        self.publish_inner(subject, trace_id, None, event).await
    }

    /// Like [`Bus::publish`], but the stream discards a second message with the same
    /// `message_id` within two minutes, so retrying a publish cannot duplicate the event.
    pub async fn publish_once<T: Serialize>(
        &self,
        subject: &str,
        trace_id: &str,
        message_id: &str,
        event: &T,
    ) -> Result<(), BusError> {
        self.publish_inner(subject, trace_id, Some(message_id), event)
            .await
    }

    async fn publish_inner<T: Serialize>(
        &self,
        subject: &str,
        trace_id: &str,
        message_id: Option<&str>,
        event: &T,
    ) -> Result<(), BusError> {
        let payload = serde_json::to_vec(event)?;
        let mut headers = HeaderMap::new();
        headers.insert(TRACE_ID_HEADER, trace_id);
        if let Some(id) = message_id {
            headers.insert(async_nats::header::NATS_MESSAGE_ID, id);
        }
        let ack = self
            .jetstream
            .publish_with_headers(subject.to_owned(), headers, payload.into())
            .await
            .map_err(|err| BusError::Publish(err.to_string()))?;
        ack.await
            .map_err(|err| BusError::Publish(err.to_string()))?;
        metrics::counter!("bus_published_total", "subject" => subject.to_owned()).increment(1);
        Ok(())
    }

    /// Durable pull consumer `name` on `stream`, receiving only `subject`. Replicas of a
    /// service share it, so each message goes to one of them.
    pub async fn durable_consumer(
        &self,
        stream: &str,
        name: &str,
        subject: &str,
        ack_wait: Duration,
    ) -> Result<PullConsumer, BusError> {
        let stream = self
            .jetstream
            .get_stream(stream)
            .await
            .map_err(|err| BusError::Consumer(err.to_string()))?;
        stream
            .get_or_create_consumer(
                name,
                PullConfig {
                    durable_name: Some(name.to_owned()),
                    filter_subject: subject.to_owned(),
                    ack_policy: AckPolicy::Explicit,
                    ack_wait,
                    max_deliver: 5,
                    ..PullConfig::default()
                },
            )
            .await
            .map_err(|err| BusError::Consumer(err.to_string()))
    }
}

/// What a handler decided about a message.
#[derive(Debug)]
pub enum Handled {
    /// Done; acknowledge it.
    Done,
    /// Failed for a reason that may pass (database down, publish failed): redeliver later.
    Retry(String),
    /// Will never succeed (unknown monitor): drop it.
    Drop(String),
}

/// A decoded message with the trace id from its header.
#[derive(Debug)]
pub struct Delivery<T> {
    /// The event.
    pub event: T,
    /// Trace id from the `x-request-id` header, or `-` when absent.
    pub trace_id: String,
    /// Subject the message was published on.
    pub subject: String,
}

/// How long a message waits before redelivery after [`Handled::Retry`].
const RETRY_DELAY: Duration = Duration::from_secs(5);
/// Pause before pulling again after the message stream fails (NATS restarting, ...).
const RESUBSCRIBE_DELAY: Duration = Duration::from_secs(2);

/// Runs `handler` on every message of `consumer`, at most `concurrency` at a time, until
/// `stop` turns `true`. In-flight messages finish before it returns.
pub async fn consume<T, H, Fut>(
    consumer: PullConsumer,
    concurrency: usize,
    stop: watch::Receiver<bool>,
    handler: H,
) -> Result<(), BusError>
where
    T: DeserializeOwned,
    H: Fn(Delivery<T>) -> Fut,
    Fut: Future<Output = Handled>,
{
    let handler = &handler;
    while !*stop.borrow() {
        let messages = match consumer.messages().await {
            Ok(messages) => messages,
            Err(err) => {
                tracing::warn!(error = %err, "cannot pull messages, retrying");
                tokio::time::sleep(RESUBSCRIBE_DELAY).await;
                continue;
            }
        };
        let mut stopped = stop.clone();
        let stop_signal = async move {
            // An error means the sender is gone, which also means stop.
            let _ = stopped.wait_for(|stop| *stop).await;
        };
        messages
            .take_until(stop_signal)
            .for_each_concurrent(concurrency, |message| async move {
                match message {
                    Ok(message) => process(message, handler).await,
                    Err(err) => tracing::warn!(error = %err, "message stream error"),
                }
            })
            .await;
        if !*stop.borrow() {
            tracing::warn!("message stream ended, pulling again");
            tokio::time::sleep(RESUBSCRIBE_DELAY).await;
        }
    }
    Ok(())
}

async fn process<T, H, Fut>(message: jetstream::Message, handler: &H)
where
    T: DeserializeOwned,
    H: Fn(Delivery<T>) -> Fut,
    Fut: Future<Output = Handled>,
{
    let subject = message.subject.to_string();
    let trace_id = message
        .headers
        .as_ref()
        .and_then(|headers| headers.get(TRACE_ID_HEADER))
        .map_or_else(|| "-".to_owned(), |value| value.as_str().to_owned());
    let span = tracing::info_span!("message", trace_id = %trace_id, subject = %subject);

    async {
        let outcome = match serde_json::from_slice::<T>(&message.payload) {
            Ok(event) => {
                handler(Delivery {
                    event,
                    trace_id,
                    subject: subject.clone(),
                })
                .await
            }
            Err(err) => Handled::Drop(format!("undecodable payload: {err}")),
        };
        let (ack, result) = match &outcome {
            Handled::Done => (AckKind::Ack, "done"),
            Handled::Retry(reason) => {
                tracing::warn!(reason = %reason, "message will be redelivered");
                (AckKind::Nak(Some(RETRY_DELAY)), "retry")
            }
            Handled::Drop(reason) => {
                tracing::error!(reason = %reason, "message dropped");
                (AckKind::Term, "dropped")
            }
        };
        metrics::counter!("bus_consumed_total", "subject" => subject.clone(), "result" => result)
            .increment(1);
        if let Err(err) = message.ack_with(ack).await {
            tracing::warn!(error = %err, "ack failed; the message will be redelivered");
        }
    }
    .instrument(span)
    .await;
}

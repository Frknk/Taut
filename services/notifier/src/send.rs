//! Delivery through each channel kind: SMTP for email, an HTTP POST for the webhook kinds.

use std::time::Duration;

use lettre::message::Mailbox;
use lettre::message::header::ContentType;
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};
use serde_json::Value;
use taut_config::NotifierSettings;
use taut_model::ChannelKind;

use crate::channels::Channel;
use crate::render::Rendered;

/// Time allowed to hand an alert to the SMTP server or the webhook.
const SEND_TIMEOUT: Duration = Duration::from_secs(10);

/// Clients for every channel kind.
pub struct Senders {
    mailer: AsyncSmtpTransport<Tokio1Executor>,
    from: Mailbox,
    http: reqwest::Client,
}

impl Senders {
    /// Builds the clients. `allow_internal` lets webhooks reach private addresses (local
    /// development, tests).
    pub fn new(settings: &NotifierSettings, allow_internal: bool) -> anyhow::Result<Self> {
        let mailer = AsyncSmtpTransport::<Tokio1Executor>::from_url(&settings.smtp_url)?
            .timeout(Some(SEND_TIMEOUT))
            .build();
        let from = settings.email_from.parse()?;
        let http = taut_net::guarded_client(allow_internal)
            .user_agent(concat!("taut-notifier/", env!("CARGO_PKG_VERSION")))
            .timeout(SEND_TIMEOUT)
            .build()?;
        Ok(Self { mailer, from, http })
    }

    /// Sends `message` through `channel`. The error is a readable cause that never contains
    /// the webhook URL, which is a secret for Discord and Slack.
    pub async fn send(&self, channel: &Channel, message: &Rendered) -> Result<(), String> {
        match channel.kind {
            ChannelKind::Email => {
                let to = channel
                    .email_to()
                    .ok_or("email channel without recipient")?;
                self.email(to, message).await
            }
            ChannelKind::Webhook => self.post(channel, &message.webhook).await,
            ChannelKind::Discord => self.post(channel, &message.discord).await,
            ChannelKind::Slack => self.post(channel, &message.slack).await,
        }
    }

    /// Sends `message` as an email to `to`.
    pub async fn email(&self, to: &str, message: &Rendered) -> Result<(), String> {
        let to: Mailbox = to
            .parse()
            .map_err(|err| format!("invalid recipient: {err}"))?;
        let email = Message::builder()
            .from(self.from.clone())
            .to(to)
            .subject(&message.subject)
            .header(ContentType::TEXT_PLAIN)
            .body(message.text.clone())
            .map_err(|err| format!("cannot build email: {err}"))?;
        self.mailer
            .send(email)
            .await
            .map(|_| ())
            .map_err(|err| format!("smtp: {err}"))
    }

    async fn post(&self, channel: &Channel, body: &Value) -> Result<(), String> {
        let url = channel.url().ok_or("webhook channel without url")?;
        let response = self
            .http
            .post(url)
            .json(body)
            .send()
            .await
            .map_err(|err| format!("webhook: {}", err.without_url()))?;
        let status = response.status();
        if status.is_success() {
            Ok(())
        } else {
            Err(format!("webhook answered {status}"))
        }
    }
}

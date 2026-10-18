//! Alert channels as the notifier reads them, and the record of what it delivered.

use serde_json::Value;
use sqlx::PgPool;
use taut_model::ChannelKind;
use uuid::Uuid;

/// An alert channel.
#[derive(Debug, Clone)]
pub struct Channel {
    /// Channel id.
    pub id: Uuid,
    /// Owner.
    pub tenant_id: Uuid,
    /// Display name.
    pub name: String,
    /// Where the alert goes.
    pub kind: ChannelKind,
    /// `{"to"}` for email, `{"url"}` for the webhook kinds.
    pub config: Value,
    /// Key of the `X-Taut-Signature` HMAC; only generic webhooks have one.
    pub signing_secret: Option<String>,
}

impl Channel {
    /// Recipient of an email channel.
    pub fn email_to(&self) -> Option<&str> {
        self.config.get("to").and_then(Value::as_str)
    }

    /// URL of a webhook channel.
    pub fn url(&self) -> Option<&str> {
        self.config.get("url").and_then(Value::as_str)
    }
}

/// Active channels of a tenant.
pub async fn active(pool: &PgPool, tenant_id: Uuid) -> Result<Vec<Channel>, sqlx::Error> {
    sqlx::query_as!(
        Channel,
        r#"
        SELECT id, tenant_id, name, kind AS "kind: ChannelKind", config, signing_secret
        FROM alert_channels
        WHERE tenant_id = $1 AND active
        ORDER BY created_at
        "#,
        tenant_id
    )
    .fetch_all(pool)
    .await
}

/// One channel of a tenant, active or not.
pub async fn find(
    pool: &PgPool,
    tenant_id: Uuid,
    id: Uuid,
) -> Result<Option<Channel>, sqlx::Error> {
    sqlx::query_as!(
        Channel,
        r#"
        SELECT id, tenant_id, name, kind AS "kind: ChannelKind", config, signing_secret
        FROM alert_channels
        WHERE tenant_id = $1 AND id = $2
        "#,
        tenant_id,
        id
    )
    .fetch_optional(pool)
    .await
}

/// Whether the alert `event` (`opened` or `resolved`) of `incident_id` already reached the
/// channel.
pub async fn already_sent(
    pool: &PgPool,
    incident_id: Uuid,
    channel_id: Uuid,
    event: &str,
) -> Result<bool, sqlx::Error> {
    let sent = sqlx::query_scalar!(
        r#"
        SELECT EXISTS (
            SELECT 1 FROM notification_deliveries
            WHERE incident_id = $1 AND channel_id = $2 AND event = $3 AND status = 'sent'
        ) AS "sent!"
        "#,
        incident_id,
        channel_id,
        event
    )
    .fetch_one(pool)
    .await?;
    Ok(sent)
}

/// Records the outcome of one delivery attempt of an incident alert.
pub async fn record_delivery(
    pool: &PgPool,
    incident_id: Uuid,
    channel: &Channel,
    event: &str,
    error: Option<&str>,
) -> Result<(), sqlx::Error> {
    let status = if error.is_none() { "sent" } else { "failed" };
    sqlx::query!(
        r#"
        INSERT INTO notification_deliveries (incident_id, channel_id, event, tenant_id, status,
                                             last_error)
        VALUES ($1, $2, $3, $4, $5, $6)
        ON CONFLICT (incident_id, channel_id, event) DO UPDATE
        SET status = EXCLUDED.status, last_error = EXCLUDED.last_error,
            attempts = notification_deliveries.attempts + 1, updated_at = now()
        "#,
        incident_id,
        channel.id,
        event,
        channel.tenant_id,
        status,
        error
    )
    .execute(pool)
    .await?;
    record_last_delivery(pool, channel.id, error).await
}

/// Stores the outcome of the last message sent through a channel, shown in the panel.
pub async fn record_last_delivery(
    pool: &PgPool,
    channel_id: Uuid,
    error: Option<&str>,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE alert_channels SET last_delivery_at = now(), last_delivery_error = $2 WHERE id = $1",
        channel_id,
        error
    )
    .execute(pool)
    .await?;
    Ok(())
}

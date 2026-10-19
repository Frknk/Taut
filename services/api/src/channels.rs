//! `/v1/alert-channels`: where a tenant's alerts go (F-40 email, F-41 webhooks).
//!
//! The API only stores channels; the notifier sends. The test button publishes
//! `notifications.test` and the notifier records the outcome in `last_delivery_*`.
//!
//! Channels decide where alerts (and their monitor names and causes) go, so every change
//! is a sensitive action: owners only, with a recent authentication (step-up).
//!
//! Generic webhooks get a signing secret (`whsec_…`) for the `X-Taut-Signature` HMAC. It is
//! returned once, when created (or when a channel becomes a webhook), and again only by
//! `POST /v1/alert-channels/{id}/rotate-secret`, which replaces it; reads only say whether
//! there is one.

// Axum handlers receive their state by value even when they only borrow it.
#![allow(clippy::needless_pass_by_value)]

use axum::Router;
use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::{get, post};
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;
use sqlx::PgPool;
use taut_http::{AppError, Json, Path, current_trace_id};
use taut_identity::webhook;
use taut_model::ChannelKind;
use taut_model::events::{TestNotification, subjects};
use uuid::Uuid;

use crate::AppState;
use crate::tenant::{CurrentTenant, Permission};
use crate::validate::{self, ChannelInput};

/// Channels a tenant may have; generous, it only stops runaway scripts.
const MAX_CHANNELS: i64 = 20;

/// Routes under `/v1/alert-channels`.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/v1/alert-channels", get(list).post(create))
        .route(
            "/v1/alert-channels/{id}",
            get(show).put(update).delete(remove),
        )
        .route("/v1/alert-channels/{id}/test", post(test))
        .route("/v1/alert-channels/{id}/rotate-secret", post(rotate_secret))
}

/// An alert channel as the API returns it.
#[derive(Debug, Clone, Serialize)]
pub struct AlertChannel {
    /// Identifier.
    pub id: Uuid,
    /// Owner.
    pub tenant_id: Uuid,
    /// Display name.
    pub name: String,
    /// Where alerts go. Serialized as `type`.
    #[serde(rename = "type")]
    pub kind: ChannelKind,
    /// `{"to"}` or `{"url"}`.
    pub config: Value,
    /// Whether alerts are sent.
    pub active: bool,
    /// Last message sent through the channel (alert or test).
    pub last_delivery_at: Option<DateTime<Utc>>,
    /// Its error, if it failed.
    pub last_delivery_error: Option<String>,
    /// Whether webhooks go out signed; the secret itself is never read back.
    pub signing_secret_set: bool,
    /// Creation time.
    pub created_at: DateTime<Utc>,
}

/// A channel after a write, with the signing secret when the write generated one: the only
/// time it is shown besides a rotation.
#[derive(Debug, Clone, Serialize)]
pub struct ChannelWithSecret {
    /// The channel.
    #[serde(flatten)]
    pub channel: AlertChannel,
    /// The new `whsec_…` secret, absent when none was generated.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signing_secret: Option<String>,
}

/// Body of a rotation: the new secret, shown once.
#[derive(Debug, Clone, Serialize)]
pub struct RotatedSecret {
    /// The new `whsec_…` secret; the previous one stops working at once.
    pub signing_secret: String,
}

/// A fresh secret when `kind` gets signed deliveries.
fn secret_for(kind: ChannelKind) -> Option<String> {
    (kind == ChannelKind::Webhook).then(webhook::generate_secret)
}

fn not_found() -> AppError {
    AppError::NotFound("alert channel not found".to_owned())
}

async fn find(pool: &PgPool, tenant_id: Uuid, id: Uuid) -> Result<Option<AlertChannel>, AppError> {
    let channel = sqlx::query_as!(
        AlertChannel,
        r#"
        SELECT id, tenant_id, name, kind AS "kind: ChannelKind", config, active,
               last_delivery_at, last_delivery_error,
               signing_secret IS NOT NULL AS "signing_secret_set!", created_at
        FROM alert_channels
        WHERE tenant_id = $1 AND id = $2
        "#,
        tenant_id,
        id,
    )
    .fetch_optional(pool)
    .await?;
    Ok(channel)
}

async fn list(
    State(state): State<AppState>,
    CurrentTenant { tenant, .. }: CurrentTenant,
) -> Result<Json<Vec<AlertChannel>>, AppError> {
    let channels = sqlx::query_as!(
        AlertChannel,
        r#"
        SELECT id, tenant_id, name, kind AS "kind: ChannelKind", config, active,
               last_delivery_at, last_delivery_error,
               signing_secret IS NOT NULL AS "signing_secret_set!", created_at
        FROM alert_channels
        WHERE tenant_id = $1
        ORDER BY created_at
        "#,
        tenant.id,
    )
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(channels))
}

async fn show(
    State(state): State<AppState>,
    CurrentTenant { tenant, .. }: CurrentTenant,
    Path(id): Path<Uuid>,
) -> Result<Json<AlertChannel>, AppError> {
    find(&state.pool, tenant.id, id)
        .await?
        .map(Json)
        .ok_or_else(not_found)
}

async fn create(
    State(state): State<AppState>,
    CurrentTenant { tenant, actor }: CurrentTenant,
    Json(input): Json<ChannelInput>,
) -> Result<(StatusCode, Json<ChannelWithSecret>), AppError> {
    actor.require(Permission::Channels)?;
    actor.require_recent_auth()?;
    let spec = validate::channel(&input, state.settings.checks.allow_private_targets)
        .map_err(AppError::BadRequest)?;
    let mut tx = state.pool.begin().await?;
    sqlx::query!("SELECT id FROM tenants WHERE id = $1 FOR UPDATE", tenant.id)
        .fetch_one(&mut *tx)
        .await?;
    let used = sqlx::query_scalar!(
        r#"SELECT count(*) AS "count!" FROM alert_channels WHERE tenant_id = $1"#,
        tenant.id
    )
    .fetch_one(&mut *tx)
    .await?;
    if used >= MAX_CHANNELS {
        return Err(AppError::Forbidden(format!(
            "a tenant can have at most {MAX_CHANNELS} alert channels"
        )));
    }
    let id = Uuid::now_v7();
    let signing_secret = secret_for(spec.kind);
    sqlx::query!(
        "INSERT INTO alert_channels (id, tenant_id, name, kind, config, active, signing_secret)
         VALUES ($1, $2, $3, $4, $5, $6, $7)",
        id,
        tenant.id,
        spec.name,
        spec.kind.as_str(),
        spec.config,
        spec.active,
        signing_secret,
    )
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    tracing::info!(channel_id = %id, kind = %spec.kind, "alert channel created");
    let channel = find(&state.pool, tenant.id, id)
        .await?
        .ok_or_else(not_found)?;
    Ok((
        StatusCode::CREATED,
        Json(ChannelWithSecret {
            channel,
            signing_secret,
        }),
    ))
}

async fn update(
    State(state): State<AppState>,
    CurrentTenant { tenant, actor }: CurrentTenant,
    Path(id): Path<Uuid>,
    Json(input): Json<ChannelInput>,
) -> Result<Json<ChannelWithSecret>, AppError> {
    actor.require(Permission::Channels)?;
    actor.require_recent_auth()?;
    let spec = validate::channel(&input, state.settings.checks.allow_private_targets)
        .map_err(AppError::BadRequest)?;
    // A webhook keeps its secret, or its lack of one: a webhook from before signing stays
    // unsigned until rotated, since the panel's mute button sends this same PUT and would never
    // show a secret generated here. A channel that becomes a webhook gets a new secret, and one
    // that stops being one loses it. In SET, `kind` and `signing_secret` are the old values.
    let candidate = secret_for(spec.kind);
    let generated = sqlx::query_scalar!(
        r#"
        UPDATE alert_channels
        SET name = $3, kind = $4, config = $5, active = $6,
            signing_secret = CASE
                WHEN $4 <> 'webhook' THEN NULL
                WHEN kind = 'webhook' THEN signing_secret
                ELSE $7
            END
        WHERE tenant_id = $1 AND id = $2
        RETURNING coalesce(signing_secret = $7, false) AS "generated!"
        "#,
        tenant.id,
        id,
        spec.name,
        spec.kind.as_str(),
        spec.config,
        spec.active,
        candidate,
    )
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(not_found)?;
    tracing::info!(channel_id = %id, "alert channel updated");
    let channel = find(&state.pool, tenant.id, id)
        .await?
        .ok_or_else(not_found)?;
    Ok(Json(ChannelWithSecret {
        channel,
        signing_secret: candidate.filter(|_| generated),
    }))
}

async fn remove(
    State(state): State<AppState>,
    CurrentTenant { tenant, actor }: CurrentTenant,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    actor.require(Permission::Channels)?;
    actor.require_recent_auth()?;
    let deleted = sqlx::query!(
        "DELETE FROM alert_channels WHERE tenant_id = $1 AND id = $2",
        tenant.id,
        id
    )
    .execute(&state.pool)
    .await?;
    if deleted.rows_affected() == 0 {
        return Err(not_found());
    }
    tracing::info!(channel_id = %id, "alert channel deleted");
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Serialize)]
struct TestQueued {
    status: &'static str,
}

async fn test(
    State(state): State<AppState>,
    CurrentTenant { tenant, actor }: CurrentTenant,
    Path(id): Path<Uuid>,
) -> Result<(StatusCode, Json<TestQueued>), AppError> {
    actor.require(Permission::Channels)?;
    actor.require_recent_auth()?;
    if find(&state.pool, tenant.id, id).await?.is_none() {
        return Err(not_found());
    }
    let request = TestNotification {
        trace_id: current_trace_id().unwrap_or_else(|| ulid::Ulid::generate().to_string()),
        tenant_id: tenant.id,
        channel_id: id,
    };
    state
        .bus
        .publish(subjects::NOTIFICATIONS_TEST, &request.trace_id, &request)
        .await
        .map_err(|err| {
            tracing::warn!(error = %err, channel_id = %id, "test notification not published");
            AppError::Unavailable("the test could not be queued; retry".to_owned())
        })?;
    Ok((StatusCode::ACCEPTED, Json(TestQueued { status: "queued" })))
}

async fn rotate_secret(
    State(state): State<AppState>,
    CurrentTenant { tenant, actor }: CurrentTenant,
    Path(id): Path<Uuid>,
) -> Result<Json<RotatedSecret>, AppError> {
    actor.require(Permission::Channels)?;
    actor.require_recent_auth()?;
    let secret = webhook::generate_secret();
    let rotated = sqlx::query!(
        "UPDATE alert_channels SET signing_secret = $3
         WHERE tenant_id = $1 AND id = $2 AND kind = 'webhook'",
        tenant.id,
        id,
        secret,
    )
    .execute(&state.pool)
    .await?;
    if rotated.rows_affected() == 0 {
        return Err(match find(&state.pool, tenant.id, id).await? {
            Some(_) => {
                AppError::BadRequest("only generic webhooks have a signing secret".to_owned())
            }
            None => not_found(),
        });
    }
    tracing::info!(channel_id = %id, "webhook signing secret rotated");
    Ok(Json(RotatedSecret {
        signing_secret: secret,
    }))
}

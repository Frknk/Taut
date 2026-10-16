//! `/v1/incidents`: the incidents of the tenant (F-32) and where their alerts went.

// Axum handlers receive their state by value even when they only borrow it.
#![allow(clippy::needless_pass_by_value)]

use axum::Router;
use axum::extract::State;
use axum::routing::get;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use taut_http::{AppError, Json, Path, Query};
use taut_model::ChannelKind;
use uuid::Uuid;

use crate::AppState;
use crate::tenant::CurrentTenant;

/// Routes under `/v1/incidents`.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/v1/incidents", get(list))
        .route("/v1/incidents/{id}", get(show))
}

/// An incident as the API returns it.
#[derive(Debug, Clone, Serialize)]
pub struct Incident {
    /// Identifier.
    pub id: Uuid,
    /// Monitor that failed.
    pub monitor_id: Uuid,
    /// Its display name.
    pub monitor_name: String,
    /// First failure of the streak that opened it.
    pub started_at: DateTime<Utc>,
    /// When it was resolved; `None` while open.
    pub resolved_at: Option<DateTime<Utc>>,
    /// Cause of the first failure.
    pub cause: String,
    /// Seconds it lasted, or has lasted so far.
    pub duration_s: i64,
}

/// One alert sent (or attempted) for an incident.
#[derive(Debug, Clone, Serialize)]
pub struct Delivery {
    /// Channel it went to.
    pub channel_id: Uuid,
    /// Channel name.
    pub channel_name: String,
    /// Channel kind. Serialized as `channel_type`.
    #[serde(rename = "channel_type")]
    pub channel_kind: ChannelKind,
    /// `opened` or `resolved`.
    pub event: String,
    /// `sent` or `failed`.
    pub status: String,
    /// Attempts so far.
    pub attempts: i32,
    /// Cause of the last failure.
    pub last_error: Option<String>,
    /// Last attempt.
    pub updated_at: DateTime<Utc>,
}

/// An incident with its deliveries.
#[derive(Debug, Clone, Serialize)]
pub struct IncidentDetail {
    /// The incident.
    #[serde(flatten)]
    pub incident: Incident,
    /// Alerts sent for it, oldest first.
    pub deliveries: Vec<Delivery>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
enum StatusFilter {
    Open,
    Resolved,
    All,
}

#[derive(Debug, Deserialize)]
struct ListQuery {
    status: Option<StatusFilter>,
    monitor_id: Option<Uuid>,
    limit: Option<i64>,
}

const DEFAULT_LIMIT: i64 = 50;
const MAX_LIMIT: i64 = 200;

async fn list(
    State(state): State<AppState>,
    CurrentTenant { tenant, .. }: CurrentTenant,
    Query(query): Query<ListQuery>,
) -> Result<Json<Vec<Incident>>, AppError> {
    let (open, resolved) = match query.status.unwrap_or(StatusFilter::All) {
        StatusFilter::Open => (true, false),
        StatusFilter::Resolved => (false, true),
        StatusFilter::All => (true, true),
    };
    let limit = query.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
    let incidents = sqlx::query_as!(
        Incident,
        r#"
        SELECT i.id, i.monitor_id, m.name AS monitor_name, i.started_at, i.resolved_at, i.cause,
               floor(extract(epoch FROM coalesce(i.resolved_at, now()) - i.started_at))::bigint
                   AS "duration_s!"
        FROM incidents i
        JOIN monitors m ON m.id = i.monitor_id
        WHERE i.tenant_id = $1
          AND ($2::uuid IS NULL OR i.monitor_id = $2)
          AND ((i.resolved_at IS NULL AND $3) OR (i.resolved_at IS NOT NULL AND $4))
        ORDER BY i.resolved_at IS NULL DESC, i.started_at DESC
        LIMIT $5
        "#,
        tenant.id,
        query.monitor_id,
        open,
        resolved,
        limit,
    )
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(incidents))
}

async fn show(
    State(state): State<AppState>,
    CurrentTenant { tenant, .. }: CurrentTenant,
    Path(id): Path<Uuid>,
) -> Result<Json<IncidentDetail>, AppError> {
    let incident = sqlx::query_as!(
        Incident,
        r#"
        SELECT i.id, i.monitor_id, m.name AS monitor_name, i.started_at, i.resolved_at, i.cause,
               floor(extract(epoch FROM coalesce(i.resolved_at, now()) - i.started_at))::bigint
                   AS "duration_s!"
        FROM incidents i
        JOIN monitors m ON m.id = i.monitor_id
        WHERE i.tenant_id = $1 AND i.id = $2
        "#,
        tenant.id,
        id,
    )
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| AppError::NotFound("incident not found".to_owned()))?;
    let deliveries = sqlx::query_as!(
        Delivery,
        r#"
        SELECT d.channel_id, c.name AS channel_name, c.kind AS "channel_kind: ChannelKind",
               d.event, d.status, d.attempts, d.last_error, d.updated_at
        FROM notification_deliveries d
        JOIN alert_channels c ON c.id = d.channel_id
        WHERE d.incident_id = $1
        ORDER BY d.updated_at
        "#,
        id,
    )
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(IncidentDetail {
        incident,
        deliveries,
    }))
}

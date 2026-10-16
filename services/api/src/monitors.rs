//! `/v1/monitors`: create, read, update, delete, pause and resume monitors.
//!
//! Covers F-22 (name), F-14 (interval), F-15 (timeout), the fields of F-16/F-17
//! (confirmation and recovery periods) and F-25 (pause/resume). Every query is scoped by the
//! tenant of the request, so a monitor of another tenant answers `404` as if it did not exist.

// Axum handlers receive their state by value even when they only borrow it.
#![allow(clippy::needless_pass_by_value)]

use axum::Router;
use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::{get, post};
use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::PgPool;
use taut_http::{AppError, Json, Path};
use taut_model::{MonitorKind, MonitorStatus};
use uuid::Uuid;

use crate::AppState;
use crate::tenant::CurrentTenant;
use crate::validate::{self, MonitorInput};

/// Routes under `/v1/monitors`.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/v1/monitors", get(list).post(create))
        .route("/v1/monitors/{id}", get(show).put(update).delete(remove))
        .route("/v1/monitors/{id}/pause", post(pause))
        .route("/v1/monitors/{id}/resume", post(resume))
}

/// A monitor as the API returns it.
#[derive(Debug, Clone, Serialize)]
pub struct Monitor {
    /// Identifier, a UUID v7.
    pub id: Uuid,
    /// Owning tenant.
    pub tenant_id: Uuid,
    /// Display name.
    pub name: String,
    /// What the monitor checks. Serialized as `type`.
    #[serde(rename = "type")]
    pub kind: MonitorKind,
    /// URL or `host:port`; `None` for heartbeats.
    pub target: Option<String>,
    /// Seconds between checks.
    pub interval_s: i32,
    /// Seconds before a check counts as failed.
    pub timeout_s: i32,
    /// HTTP status that counts as healthy; `None` means any 2xx.
    pub expected_status: Option<i16>,
    /// Text the response body must contain (`keyword`).
    pub keyword: Option<String>,
    /// Seconds a failure must persist before an incident opens.
    pub confirmation_period_s: i32,
    /// Seconds of consecutive successes before an incident closes.
    pub recovery_period_s: i32,
    /// Heartbeats: extra seconds of silence tolerated after the interval.
    pub grace_period_s: i32,
    /// Heartbeats: secret path segment of the ping URL.
    pub heartbeat_token: Option<String>,
    /// Heartbeats: last ping received.
    pub last_ping_at: Option<DateTime<Utc>>,
    /// `false` while paused.
    pub active: bool,
    /// Health as shown to users.
    pub status: MonitorStatus,
    /// Creation time.
    pub created_at: DateTime<Utc>,
    /// Last configuration change.
    pub updated_at: DateTime<Utc>,
}

fn not_found() -> AppError {
    AppError::NotFound("monitor not found".to_owned())
}

/// Reads one monitor of `tenant_id`.
pub async fn find(pool: &PgPool, tenant_id: Uuid, id: Uuid) -> Result<Option<Monitor>, AppError> {
    let monitor = sqlx::query_as!(
        Monitor,
        r#"
        SELECT id, tenant_id, name, kind AS "kind: MonitorKind", target, interval_s, timeout_s,
               expected_status, keyword, confirmation_period_s, recovery_period_s,
               grace_period_s, heartbeat_token, last_ping_at, active,
               CASE WHEN active THEN 'pending' ELSE 'paused' END AS "status!: MonitorStatus",
               created_at, updated_at
        FROM monitors
        WHERE tenant_id = $1 AND id = $2
        "#,
        tenant_id,
        id,
    )
    .fetch_optional(pool)
    .await?;
    Ok(monitor)
}

async fn list(
    State(state): State<AppState>,
    CurrentTenant(tenant): CurrentTenant,
) -> Result<Json<Vec<Monitor>>, AppError> {
    let monitors = sqlx::query_as!(
        Monitor,
        r#"
        SELECT id, tenant_id, name, kind AS "kind: MonitorKind", target, interval_s, timeout_s,
               expected_status, keyword, confirmation_period_s, recovery_period_s,
               grace_period_s, heartbeat_token, last_ping_at, active,
               CASE WHEN active THEN 'pending' ELSE 'paused' END AS "status!: MonitorStatus",
               created_at, updated_at
        FROM monitors
        WHERE tenant_id = $1
        ORDER BY created_at
        "#,
        tenant.id,
    )
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(monitors))
}

async fn show(
    State(state): State<AppState>,
    CurrentTenant(tenant): CurrentTenant,
    Path(id): Path<Uuid>,
) -> Result<Json<Monitor>, AppError> {
    find(&state.pool, tenant.id, id)
        .await?
        .map(Json)
        .ok_or_else(not_found)
}

async fn create(
    State(state): State<AppState>,
    CurrentTenant(tenant): CurrentTenant,
    Json(input): Json<MonitorInput>,
) -> Result<(StatusCode, Json<Monitor>), AppError> {
    let spec = validate::monitor(input, state.settings.checks.allow_private_targets)
        .map_err(AppError::BadRequest)?;
    let heartbeat = spec.kind == MonitorKind::Heartbeat;

    let mut tx = state.pool.begin().await?;
    // Locking the tenant row serializes concurrent creations, so two requests cannot both
    // pass the quota check below.
    sqlx::query!("SELECT id FROM tenants WHERE id = $1 FOR UPDATE", tenant.id)
        .fetch_one(&mut *tx)
        .await?;
    let used = sqlx::query_scalar!(
        r#"SELECT count(*) AS "count!" FROM monitors WHERE tenant_id = $1 AND (kind = 'heartbeat') = $2"#,
        tenant.id,
        heartbeat,
    )
    .fetch_one(&mut *tx)
    .await?;
    let (quota, what) = if heartbeat {
        (tenant.plan.heartbeat_quota(), "heartbeats")
    } else {
        (tenant.plan.monitor_quota(), "monitors")
    };
    if used >= quota {
        return Err(AppError::Forbidden(format!(
            "the {} plan allows {quota} {what}; delete one before creating another",
            tenant.plan.as_str()
        )));
    }

    let id = Uuid::now_v7();
    // 122 random bits: the token is the only secret of the public ping URL.
    let heartbeat_token = heartbeat.then(|| Uuid::new_v4().simple().to_string());
    sqlx::query!(
        r#"
        INSERT INTO monitors (id, tenant_id, name, kind, target, interval_s, timeout_s,
                              expected_status, keyword, confirmation_period_s,
                              recovery_period_s, grace_period_s, heartbeat_token)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
        "#,
        id,
        tenant.id,
        spec.name,
        spec.kind.as_str(),
        spec.target,
        spec.interval_s,
        spec.timeout_s,
        spec.expected_status,
        spec.keyword,
        spec.confirmation_period_s,
        spec.recovery_period_s,
        spec.grace_period_s,
        heartbeat_token,
    )
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;

    tracing::info!(monitor_id = %id, kind = %spec.kind, "monitor created");
    let monitor = find(&state.pool, tenant.id, id)
        .await?
        .ok_or_else(not_found)?;
    Ok((StatusCode::CREATED, Json(monitor)))
}

async fn update(
    State(state): State<AppState>,
    CurrentTenant(tenant): CurrentTenant,
    Path(id): Path<Uuid>,
    Json(input): Json<MonitorInput>,
) -> Result<Json<Monitor>, AppError> {
    let spec = validate::monitor(input, state.settings.checks.allow_private_targets)
        .map_err(AppError::BadRequest)?;
    let current = find(&state.pool, tenant.id, id)
        .await?
        .ok_or_else(not_found)?;
    if current.kind != spec.kind {
        return Err(AppError::BadRequest(
            "type cannot be changed; create a new monitor instead".to_owned(),
        ));
    }

    let updated = sqlx::query!(
        r#"
        UPDATE monitors
        SET name = $3, target = $4, interval_s = $5, timeout_s = $6, expected_status = $7,
            keyword = $8, confirmation_period_s = $9, recovery_period_s = $10,
            grace_period_s = $11, updated_at = now()
        WHERE tenant_id = $1 AND id = $2
        "#,
        tenant.id,
        id,
        spec.name,
        spec.target,
        spec.interval_s,
        spec.timeout_s,
        spec.expected_status,
        spec.keyword,
        spec.confirmation_period_s,
        spec.recovery_period_s,
        spec.grace_period_s,
    )
    .execute(&state.pool)
    .await?;
    if updated.rows_affected() == 0 {
        return Err(not_found());
    }

    tracing::info!(monitor_id = %id, "monitor updated");
    find(&state.pool, tenant.id, id)
        .await?
        .map(Json)
        .ok_or_else(not_found)
}

async fn remove(
    State(state): State<AppState>,
    CurrentTenant(tenant): CurrentTenant,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    let deleted = sqlx::query!(
        "DELETE FROM monitors WHERE tenant_id = $1 AND id = $2",
        tenant.id,
        id
    )
    .execute(&state.pool)
    .await?;
    if deleted.rows_affected() == 0 {
        return Err(not_found());
    }
    tracing::info!(monitor_id = %id, "monitor deleted");
    Ok(StatusCode::NO_CONTENT)
}

async fn pause(
    State(state): State<AppState>,
    CurrentTenant(tenant): CurrentTenant,
    Path(id): Path<Uuid>,
) -> Result<Json<Monitor>, AppError> {
    set_active(&state, tenant.id, id, false).await
}

async fn resume(
    State(state): State<AppState>,
    CurrentTenant(tenant): CurrentTenant,
    Path(id): Path<Uuid>,
) -> Result<Json<Monitor>, AppError> {
    set_active(&state, tenant.id, id, true).await
}

async fn set_active(
    state: &AppState,
    tenant_id: Uuid,
    id: Uuid,
    active: bool,
) -> Result<Json<Monitor>, AppError> {
    let updated = sqlx::query!(
        "UPDATE monitors SET active = $3, updated_at = now() WHERE tenant_id = $1 AND id = $2",
        tenant_id,
        id,
        active,
    )
    .execute(&state.pool)
    .await?;
    if updated.rows_affected() == 0 {
        return Err(not_found());
    }
    tracing::info!(monitor_id = %id, active, "monitor active flag changed");
    find(&state.pool, tenant_id, id)
        .await?
        .map(Json)
        .ok_or_else(not_found)
}

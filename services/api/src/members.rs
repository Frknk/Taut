//! `/v1/members`: who belongs to the current tenant. Read-only until invitations arrive.

// Axum handlers receive their state by value even when they only borrow it.
#![allow(clippy::needless_pass_by_value)]

use axum::Router;
use axum::extract::State;
use axum::routing::get;
use chrono::{DateTime, Utc};
use serde::Serialize;
use taut_http::{AppError, Json};
use uuid::Uuid;

use crate::AppState;
use crate::tenant::{CurrentTenant, Role};

/// Routes under `/v1/members`.
pub fn router() -> Router<AppState> {
    Router::new().route("/v1/members", get(list))
}

/// A member of the tenant, as the API returns it.
#[derive(Debug, Clone, Serialize)]
pub struct Member {
    /// User id.
    pub user_id: Uuid,
    /// Display name.
    pub name: String,
    /// Address.
    pub email: String,
    /// Role in the tenant.
    pub role: Role,
    /// When they joined.
    pub joined_at: DateTime<Utc>,
}

async fn list(
    State(state): State<AppState>,
    CurrentTenant { tenant, .. }: CurrentTenant,
) -> Result<Json<Vec<Member>>, AppError> {
    let members = sqlx::query_as!(
        Member,
        r#"
        SELECT u.id AS user_id, u.name, u.email, m.role AS "role: Role", m.created_at AS joined_at
        FROM memberships m
        JOIN users u ON u.id = m.user_id
        WHERE m.tenant_id = $1
        ORDER BY m.created_at, u.email
        "#,
        tenant.id,
    )
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(members))
}

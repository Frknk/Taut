//! `/v1/members` and `/v1/tenant`: who belongs to the current tenant, and the tenant itself.
//!
//! Owners change roles, remove members, rename and delete the tenant; any member can leave.
//! A tenant always keeps at least one owner: demoting, removing or the leaving of the last one
//! answers `409`. [`last_owner`] locks the tenant row first, so two owners demoting each other
//! at the same time cannot both succeed.
//!
//! Deleting the tenant is a hard delete with step-up: its check results go explicitly (the
//! hypertable has no foreign key) and the rest cascades from `tenants`. The scheduler drops
//! the monitors on its next resync.

// Axum handlers receive their state by value even when they only borrow it.
#![allow(clippy::needless_pass_by_value)]

use axum::Router;
use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::{get, post, put};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgConnection;
use taut_http::{AppError, Json, Path};
use uuid::Uuid;

use crate::AppState;
use crate::tenant::{CurrentTenant, Permission, Plan, Role, Tenant};
use crate::tenants;

/// Routes under `/v1/members` and `/v1/tenant`.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/v1/members", get(list))
        .route("/v1/members/{user_id}", put(change_role).delete(remove))
        .route("/v1/tenant", get(show).put(rename).delete(delete_tenant))
        .route("/v1/tenant/leave", post(leave))
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

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RoleChange {
    role: Role,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Rename {
    name: String,
}

fn member_not_found() -> AppError {
    AppError::NotFound("member not found".to_owned())
}

fn needs_an_owner() -> AppError {
    AppError::Conflict("the tenant needs at least one owner".to_owned())
}

/// Locks the tenant row for the rest of the transaction and returns its only owner, or
/// `None` when it has several (or, transiently, none).
async fn last_owner(tx: &mut PgConnection, tenant_id: Uuid) -> Result<Option<Uuid>, AppError> {
    sqlx::query!("SELECT id FROM tenants WHERE id = $1 FOR UPDATE", tenant_id)
        .fetch_one(&mut *tx)
        .await?;
    let owners = sqlx::query_scalar!(
        "SELECT user_id FROM memberships WHERE tenant_id = $1 AND role = 'owner' LIMIT 2",
        tenant_id,
    )
    .fetch_all(&mut *tx)
    .await?;
    Ok(match owners.as_slice() {
        [only] => Some(*only),
        _ => None,
    })
}

async fn role_of(
    tx: &mut PgConnection,
    tenant_id: Uuid,
    user_id: Uuid,
) -> Result<Option<Role>, AppError> {
    let role = sqlx::query_scalar!(
        r#"SELECT role AS "role: Role" FROM memberships WHERE tenant_id = $1 AND user_id = $2"#,
        tenant_id,
        user_id,
    )
    .fetch_optional(&mut *tx)
    .await?;
    Ok(role)
}

/// Deletes the membership of `user_id` unless they are the last owner.
async fn drop_membership(state: &AppState, tenant_id: Uuid, user_id: Uuid) -> Result<(), AppError> {
    let mut tx = state.pool.begin().await?;
    let sole_owner = last_owner(&mut tx, tenant_id).await?;
    role_of(&mut tx, tenant_id, user_id)
        .await?
        .ok_or_else(member_not_found)?;
    if sole_owner == Some(user_id) {
        return Err(needs_an_owner());
    }
    sqlx::query!(
        "DELETE FROM memberships WHERE tenant_id = $1 AND user_id = $2",
        tenant_id,
        user_id,
    )
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
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

async fn change_role(
    State(state): State<AppState>,
    CurrentTenant { tenant, actor }: CurrentTenant,
    Path(user_id): Path<Uuid>,
    Json(input): Json<RoleChange>,
) -> Result<Json<Member>, AppError> {
    actor.require(Permission::Tenant)?;
    let mut tx = state.pool.begin().await?;
    let sole_owner = last_owner(&mut tx, tenant.id).await?;
    let current = role_of(&mut tx, tenant.id, user_id)
        .await?
        .ok_or_else(member_not_found)?;
    if current == Role::Owner && input.role == Role::Member && sole_owner == Some(user_id) {
        return Err(needs_an_owner());
    }
    let member = sqlx::query_as!(
        Member,
        r#"
        UPDATE memberships m SET role = $3
        FROM users u
        WHERE m.tenant_id = $1 AND m.user_id = $2 AND u.id = m.user_id
        RETURNING u.id AS user_id, u.name, u.email, m.role AS "role: Role",
                  m.created_at AS joined_at
        "#,
        tenant.id,
        user_id,
        input.role.as_str(),
    )
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;
    tracing::info!(member_id = %user_id, role = input.role.as_str(), "member role changed");
    Ok(Json(member))
}

async fn remove(
    State(state): State<AppState>,
    CurrentTenant { tenant, actor }: CurrentTenant,
    Path(user_id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    actor.require(Permission::Tenant)?;
    drop_membership(&state, tenant.id, user_id).await?;
    tracing::info!(member_id = %user_id, "member removed");
    Ok(StatusCode::NO_CONTENT)
}

async fn leave(
    State(state): State<AppState>,
    CurrentTenant { tenant, actor }: CurrentTenant,
) -> Result<StatusCode, AppError> {
    actor.require(Permission::Read)?;
    drop_membership(&state, tenant.id, actor.user_id()).await?;
    tracing::info!("member left the tenant");
    Ok(StatusCode::NO_CONTENT)
}

#[allow(clippy::unused_async)] // axum handlers must be async
async fn show(CurrentTenant { tenant, .. }: CurrentTenant) -> Json<Tenant> {
    Json(tenant)
}

async fn rename(
    State(state): State<AppState>,
    CurrentTenant { tenant, actor }: CurrentTenant,
    Json(input): Json<Rename>,
) -> Result<Json<Tenant>, AppError> {
    actor.require(Permission::Tenant)?;
    let name = tenants::validated_name(&input.name)?;
    let renamed = sqlx::query_as!(
        Tenant,
        r#"
        UPDATE tenants SET name = $2 WHERE id = $1
        RETURNING id, slug, name, plan AS "plan: Plan", created_at
        "#,
        tenant.id,
        name,
    )
    .fetch_one(&state.pool)
    .await?;
    tracing::info!("tenant renamed");
    Ok(Json(renamed))
}

async fn delete_tenant(
    State(state): State<AppState>,
    CurrentTenant { tenant, actor }: CurrentTenant,
) -> Result<StatusCode, AppError> {
    actor.require(Permission::Tenant)?;
    actor.require_recent_auth()?;
    let mut tx = state.pool.begin().await?;
    sqlx::query!("SELECT id FROM tenants WHERE id = $1 FOR UPDATE", tenant.id)
        .fetch_one(&mut *tx)
        .await?;
    // check_results has no foreign key (it is a hypertable), so its rows go explicitly.
    sqlx::query!("DELETE FROM check_results WHERE tenant_id = $1", tenant.id)
        .execute(&mut *tx)
        .await?;
    // Monitors, incidents, channels, memberships and invitations cascade.
    sqlx::query!("DELETE FROM tenants WHERE id = $1", tenant.id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    tracing::info!(slug = %tenant.slug, "tenant deleted");
    Ok(StatusCode::NO_CONTENT)
}

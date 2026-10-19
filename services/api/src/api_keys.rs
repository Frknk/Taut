//! `/v1/api-keys`: tenant API keys for scripts and CI (`Authorization: Bearer taut_live_…`).
//!
//! Owners only. Creating a key is a sensitive action (step-up) and the response is the only
//! time the full key is shown: the database keeps its SHA-256 and a display prefix. Revoking
//! needs no step-up, so a leaked key can be stopped at once; the row stays with `revoked_at`.
//! What a key may do once created is decided in [`crate::tenant`].

// Axum handlers receive their state by value even when they only borrow it.
#![allow(clippy::needless_pass_by_value)]

use axum::Router;
use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::{delete, get};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use taut_http::{AppError, Json, Path};
use taut_identity::api_key::{self, Scope};
use uuid::Uuid;

use crate::AppState;
use crate::tenant::{CurrentTenant, Permission};

/// Active keys a tenant may have; generous, it only stops runaway scripts.
const MAX_ACTIVE_KEYS: i64 = 20;

const MAX_NAME_CHARS: usize = 100;

/// Routes under `/v1/api-keys`.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/v1/api-keys", get(list).post(create))
        .route("/v1/api-keys/{id}", delete(revoke))
}

/// An API key as the API returns it: never the key itself.
#[derive(Debug, Clone, Serialize)]
pub struct ApiKey {
    /// Identifier.
    pub id: Uuid,
    /// What it is for, chosen by the owner.
    pub name: String,
    /// First characters of the key (`taut_live_Ab12cd34`), to tell keys apart.
    pub prefix: String,
    /// What it may do.
    pub scope: Scope,
    /// Creation time.
    pub created_at: DateTime<Utc>,
    /// Last request made with it, to the minute.
    pub last_used_at: Option<DateTime<Utc>>,
    /// When it was revoked; it no longer works after that.
    pub revoked_at: Option<DateTime<Utc>>,
}

/// Body of `201 Created`: the key plus its full value, shown this one time.
#[derive(Debug, Serialize)]
pub struct CreatedApiKey {
    /// The stored key.
    #[serde(flatten)]
    pub api_key: ApiKey,
    /// The full key (`taut_live_…`).
    pub key: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct NewApiKey {
    name: String,
    scope: Scope,
}

fn parse_scope(value: &str) -> Result<Scope, AppError> {
    Ok(value.parse().map_err(anyhow::Error::from)?)
}

async fn list(
    State(state): State<AppState>,
    CurrentTenant { tenant, actor }: CurrentTenant,
) -> Result<Json<Vec<ApiKey>>, AppError> {
    actor.require(Permission::Tenant)?;
    let mut tx = state.tenant_tx(tenant.id).await?;
    let rows = sqlx::query!(
        r#"
        SELECT id, name, prefix, scope, created_at, last_used_at, revoked_at
        FROM api_keys
        WHERE tenant_id = $1
        ORDER BY revoked_at IS NOT NULL, created_at DESC
        "#,
        tenant.id,
    )
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;
    let keys = rows
        .into_iter()
        .map(|row| {
            Ok(ApiKey {
                id: row.id,
                name: row.name,
                prefix: row.prefix,
                scope: parse_scope(&row.scope)?,
                created_at: row.created_at,
                last_used_at: row.last_used_at,
                revoked_at: row.revoked_at,
            })
        })
        .collect::<Result<_, AppError>>()?;
    Ok(Json(keys))
}

async fn create(
    State(state): State<AppState>,
    CurrentTenant { tenant, actor }: CurrentTenant,
    Json(input): Json<NewApiKey>,
) -> Result<(StatusCode, Json<CreatedApiKey>), AppError> {
    actor.require(Permission::Tenant)?;
    actor.require_recent_auth()?;
    let created_by = actor.user_id()?;
    let name = input.name.trim();
    if name.is_empty() || name.chars().count() > MAX_NAME_CHARS {
        return Err(AppError::BadRequest(format!(
            "name must be between 1 and {MAX_NAME_CHARS} characters"
        )));
    }

    let mut tx = state.tenant_tx(tenant.id).await?;
    sqlx::query!("SELECT id FROM tenants WHERE id = $1 FOR UPDATE", tenant.id)
        .fetch_one(&mut *tx)
        .await?;
    let active = sqlx::query_scalar!(
        r#"SELECT count(*) AS "count!" FROM api_keys WHERE tenant_id = $1 AND revoked_at IS NULL"#,
        tenant.id
    )
    .fetch_one(&mut *tx)
    .await?;
    if active >= MAX_ACTIVE_KEYS {
        return Err(AppError::Forbidden(format!(
            "a tenant can have at most {MAX_ACTIVE_KEYS} active api keys"
        )));
    }
    let generated = api_key::generate();
    let row = sqlx::query!(
        r#"
        INSERT INTO api_keys (id, tenant_id, name, prefix, key_hash, scope, created_by)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        RETURNING id, created_at
        "#,
        Uuid::now_v7(),
        tenant.id,
        name,
        generated.display_prefix,
        generated.hash,
        input.scope.as_str(),
        created_by,
    )
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;
    tracing::info!(api_key_id = %row.id, scope = %input.scope, "api key created");

    Ok((
        StatusCode::CREATED,
        Json(CreatedApiKey {
            api_key: ApiKey {
                id: row.id,
                name: name.to_owned(),
                prefix: generated.display_prefix,
                scope: input.scope,
                created_at: row.created_at,
                last_used_at: None,
                revoked_at: None,
            },
            key: generated.plain,
        }),
    ))
}

async fn revoke(
    State(state): State<AppState>,
    CurrentTenant { tenant, actor }: CurrentTenant,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    actor.require(Permission::Tenant)?;
    // Revoking twice keeps the first date.
    let mut tx = state.tenant_tx(tenant.id).await?;
    let revoked = sqlx::query_scalar!(
        r#"
        UPDATE api_keys SET revoked_at = coalesce(revoked_at, now())
        WHERE tenant_id = $1 AND id = $2
        RETURNING id
        "#,
        tenant.id,
        id,
    )
    .fetch_optional(&mut *tx)
    .await?;
    if revoked.is_none() {
        return Err(AppError::NotFound("api key not found".to_owned()));
    }
    tx.commit().await?;
    tracing::info!(api_key_id = %id, "api key revoked");
    Ok(StatusCode::NO_CONTENT)
}

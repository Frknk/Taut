//! Tenants and the extractor that resolves the tenant of a request.
//!
//! Until authentication exists (weeks 13–14) the tenant travels in the `X-Tenant-Id` header.
//! Handlers never read the header themselves: they take a [`CurrentTenant`], so moving the
//! tenant into the JWT later only changes this module.

use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use taut_http::AppError;
use taut_http::layers::TENANT_ID_HEADER;
use uuid::Uuid;

use crate::AppState;

/// Commercial plan of a tenant; sets its quotas.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[serde(rename_all = "lowercase")]
#[sqlx(type_name = "text", rename_all = "lowercase")]
pub enum Plan {
    /// The only plan of the prototype.
    Free,
}

impl Plan {
    /// Lowercase name, as stored and serialized.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Free => "free",
        }
    }

    /// Maximum number of monitors that are not heartbeats.
    pub const fn monitor_quota(self) -> i64 {
        match self {
            Self::Free => 10,
        }
    }

    /// Maximum number of heartbeat monitors.
    pub const fn heartbeat_quota(self) -> i64 {
        match self {
            Self::Free => 10,
        }
    }
}

/// A customer organization. Every business row belongs to exactly one.
#[derive(Debug, Clone, Serialize)]
pub struct Tenant {
    /// Identifier, a UUID v7.
    pub id: Uuid,
    /// URL-safe name of the public status page.
    pub slug: String,
    /// Display name.
    pub name: String,
    /// Commercial plan.
    pub plan: Plan,
    /// Creation time.
    pub created_at: DateTime<Utc>,
}

/// Looks a tenant up by id.
pub async fn find(pool: &PgPool, id: Uuid) -> Result<Option<Tenant>, sqlx::Error> {
    sqlx::query_as!(
        Tenant,
        r#"SELECT id, slug, name, plan AS "plan: Plan", created_at FROM tenants WHERE id = $1"#,
        id
    )
    .fetch_optional(pool)
    .await
}

/// The tenant a request acts on, resolved from the `X-Tenant-Id` header.
///
/// Rejects with `401 Unauthorized` when the header is missing, malformed or names a tenant
/// that does not exist. On success the tenant id is recorded on the request span, so every
/// log line of the request carries it.
#[derive(Debug, Clone)]
pub struct CurrentTenant(pub Tenant);

impl FromRequestParts<AppState> for CurrentTenant {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, AppError> {
        let header = parts
            .headers
            .get(TENANT_ID_HEADER)
            .ok_or_else(|| AppError::Unauthorized(format!("missing {TENANT_ID_HEADER} header")))?;
        let id = header
            .to_str()
            .ok()
            .and_then(|value| Uuid::parse_str(value.trim()).ok())
            .ok_or_else(|| {
                AppError::Unauthorized(format!("{TENANT_ID_HEADER} must be a tenant id"))
            })?;
        let tenant = find(&state.pool, id)
            .await?
            .ok_or_else(|| AppError::Unauthorized("unknown tenant".to_owned()))?;
        tracing::Span::current().record("tenant_id", tracing::field::display(tenant.id));
        Ok(Self(tenant))
    }
}

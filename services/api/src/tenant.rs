//! Tenants, and the extractors that say who calls and on which tenant.
//!
//! The bearer token (signed by the auth service) says who the user is; the `X-Tenant-Id`
//! header says which of their tenants the request acts on; the `memberships` table joins
//! the two and gives the role. Handlers take a [`CurrentTenant`] and ask its [`Actor`] for the
//! [`Permission`] they need before writing anything; the sensitive ones also ask for a recent
//! authentication ([`Actor::require_recent_auth`]).

use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use taut_http::AppError;
use taut_http::layers::TENANT_ID_HEADER;
use taut_identity::Claims;
use taut_identity::bearer::{self, Credential};
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

/// What a user is to a tenant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[serde(rename_all = "lowercase")]
#[sqlx(type_name = "text", rename_all = "lowercase")]
pub enum Role {
    /// Manages members, alert channels, API keys and the tenant itself.
    Owner,
    /// Manages monitors; reads everything else.
    Member,
}

impl Role {
    /// Lowercase name, as stored and serialized.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Owner => "owner",
            Self::Member => "member",
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

/// A tenant seen by one of its members: the tenant plus their role in it.
#[derive(Debug, Clone, Serialize)]
pub struct TenantWithRole {
    /// The tenant.
    #[serde(flatten)]
    pub tenant: Tenant,
    /// The caller's role.
    pub role: Role,
}

/// Looks a tenant up by id, with the role of `user_id` in it; `None` unless they are a member.
pub async fn find_for_member(
    pool: &PgPool,
    tenant_id: Uuid,
    user_id: Uuid,
) -> Result<Option<TenantWithRole>, sqlx::Error> {
    let row = sqlx::query!(
        r#"
        SELECT t.id, t.slug, t.name, t.plan AS "plan: Plan", t.created_at, m.role AS "role: Role"
        FROM memberships m
        JOIN tenants t ON t.id = m.tenant_id
        WHERE m.tenant_id = $1 AND m.user_id = $2
        "#,
        tenant_id,
        user_id,
    )
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|row| TenantWithRole {
        tenant: Tenant {
            id: row.id,
            slug: row.slug,
            name: row.name,
            plan: row.plan,
            created_at: row.created_at,
        },
        role: row.role,
    }))
}

/// What a request wants to do; the actor's role decides whether it may.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Permission {
    /// Read anything of the tenant.
    Read,
    /// Create, change, pause or delete monitors.
    Monitors,
    /// Create, change, test or delete alert channels.
    Channels,
    /// Members, invitations, API keys and the tenant itself.
    Tenant,
}

/// Who makes a request, once verified.
#[derive(Debug, Clone)]
pub enum Actor {
    /// A signed-in user, member of the tenant.
    User {
        /// User id (`sub`).
        user_id: Uuid,
        /// Role in this tenant.
        role: Role,
        /// Whether the session passed a second factor.
        mfa: bool,
        /// When the user last proved who they are.
        auth_time: DateTime<Utc>,
    },
}

impl Actor {
    /// Fails with `403` unless this actor may do `permission` here.
    pub fn require(&self, permission: Permission) -> Result<(), AppError> {
        let Self::User { role, .. } = self;
        let allowed = match permission {
            Permission::Read | Permission::Monitors => true,
            Permission::Channels | Permission::Tenant => *role == Role::Owner,
        };
        if allowed {
            Ok(())
        } else {
            Err(AppError::Forbidden("owner role required".to_owned()))
        }
    }

    /// Fails with `403 reauth_required` unless the user confirmed their password within the
    /// last five minutes (`POST /v1/auth/reauth` on the auth service). For the actions a
    /// stolen session must not be enough for: alert channels, API keys, deleting the tenant.
    pub fn require_recent_auth(&self) -> Result<(), AppError> {
        let Self::User { auth_time, .. } = self;
        let age = Utc::now().signed_duration_since(*auth_time).num_seconds();
        if age <= taut_identity::claims::RECENT_AUTH_SECS {
            Ok(())
        } else {
            Err(AppError::Denied {
                code: "reauth_required",
                message: "confirm your password to continue".to_owned(),
            })
        }
    }

    /// The user behind the request.
    pub fn user_id(&self) -> Uuid {
        let Self::User { user_id, .. } = self;
        *user_id
    }
}

/// The signed-in user, from the bearer token alone. For routes that are not about one tenant
/// (listing or creating tenants).
#[derive(Debug, Clone)]
pub struct CurrentUser {
    /// User id.
    pub user_id: Uuid,
    /// The token's claims.
    pub claims: Claims,
}

impl FromRequestParts<AppState> for CurrentUser {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, AppError> {
        let token = match bearer::credential(&parts.headers) {
            Some(Credential::Jwt(token)) => token,
            Some(Credential::ApiKey(_)) => {
                return Err(AppError::Unauthorized(
                    "api keys are not accepted yet".to_owned(),
                ));
            }
            None => return Err(AppError::Unauthorized("missing bearer token".to_owned())),
        };
        let claims = state
            .verifier
            .verify_access(token)
            .await
            .map_err(|err| AppError::Unauthorized(err.to_string()))?;
        tracing::Span::current().record("user_id", tracing::field::display(claims.sub));
        Ok(Self {
            user_id: claims.sub,
            claims,
        })
    }
}

/// The tenant a request acts on and who acts: a valid bearer token, the `X-Tenant-Id`
/// header and a membership joining them.
///
/// `401` without a valid token, `400` without a well-formed header, `403` with code
/// `not_a_member` when the user does not belong to that tenant (or it does not exist: the
/// answer never tells which). On success the tenant id is recorded on the request span.
#[derive(Debug, Clone)]
pub struct CurrentTenant {
    /// The tenant.
    pub tenant: Tenant,
    /// Who acts.
    pub actor: Actor,
}

impl FromRequestParts<AppState> for CurrentTenant {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, AppError> {
        let user = CurrentUser::from_request_parts(parts, state).await?;
        let header = parts
            .headers
            .get(TENANT_ID_HEADER)
            .ok_or_else(|| AppError::BadRequest(format!("missing {TENANT_ID_HEADER} header")))?;
        let id = header
            .to_str()
            .ok()
            .and_then(|value| Uuid::parse_str(value.trim()).ok())
            .ok_or_else(|| {
                AppError::BadRequest(format!("{TENANT_ID_HEADER} must be a tenant id"))
            })?;
        let found = find_for_member(&state.pool, id, user.user_id)
            .await?
            .ok_or_else(|| AppError::Denied {
                code: "not_a_member",
                message: "you are not a member of this tenant".to_owned(),
            })?;
        tracing::Span::current().record("tenant_id", tracing::field::display(found.tenant.id));
        Ok(Self {
            tenant: found.tenant,
            actor: Actor::User {
                user_id: user.user_id,
                role: found.role,
                mfa: user.claims.mfa,
                auth_time: user.claims.authenticated_at(),
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn actor(role: Role) -> Actor {
        Actor::User {
            user_id: Uuid::nil(),
            role,
            mfa: false,
            auth_time: Utc::now(),
        }
    }

    #[test]
    fn recent_auth_expires_after_five_minutes() {
        assert!(actor(Role::Owner).require_recent_auth().is_ok());
        let stale = Actor::User {
            user_id: Uuid::nil(),
            role: Role::Owner,
            mfa: true,
            auth_time: Utc::now() - chrono::Duration::seconds(301),
        };
        let err = stale.require_recent_auth().unwrap_err();
        assert_eq!(err.code(), "reauth_required");
    }

    #[test]
    fn members_manage_monitors_and_owners_everything() {
        let member = actor(Role::Member);
        assert!(member.require(Permission::Read).is_ok());
        assert!(member.require(Permission::Monitors).is_ok());
        assert!(member.require(Permission::Channels).is_err());
        assert!(member.require(Permission::Tenant).is_err());
        let owner = actor(Role::Owner);
        for permission in [
            Permission::Read,
            Permission::Monitors,
            Permission::Channels,
            Permission::Tenant,
        ] {
            assert!(owner.require(permission).is_ok(), "{permission:?}");
        }
    }
}

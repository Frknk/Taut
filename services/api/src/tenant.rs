//! Tenants, and the extractors that say who calls and on which tenant.
//!
//! The bearer token (signed by the auth service) says who the user is; the `X-Tenant-Id`
//! header says which of their tenants the request acts on; the `memberships` table joins
//! the two and gives the role. Handlers take a [`CurrentTenant`] and ask its [`Actor`] for the
//! [`Permission`] they need before writing anything; the sensitive ones also ask for a recent
//! authentication ([`Actor::require_recent_auth`]).
//!
//! A bearer that starts with `taut_live_` is a tenant API key instead: its tenant comes from
//! the key (the header is optional and must match), and it only reaches the monitoring routes
//! ([`API_KEY_ROUTES`]). Members, invitations, alert channels, API keys and the tenant itself
//! are for people.

use axum::extract::FromRequestParts;
use axum::http::Method;
use axum::http::request::Parts;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use taut_http::AppError;
use taut_http::layers::TENANT_ID_HEADER;
use taut_identity::Claims;
use taut_identity::api_key::{self, Scope};
use taut_identity::bearer::{self, Credential};
use uuid::Uuid;

use crate::AppState;
use crate::ratelimit::Bucket;

/// Commercial plan of a tenant; sets its quotas (docs/adr/0012). There is no billing yet: new
/// tenants start on `free` and an operator assigns the paid plans (`just tenant-plan`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[serde(rename_all = "lowercase")]
#[sqlx(type_name = "text", rename_all = "lowercase")]
pub enum Plan {
    /// The default plan of every new tenant.
    Free,
    /// First paid plan.
    Pro,
    /// Largest plan.
    Business,
}

impl Plan {
    /// Every plan, from the smallest to the largest.
    pub const ALL: [Self; 3] = [Self::Free, Self::Pro, Self::Business];

    /// Lowercase name, as stored and serialized.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Free => "free",
            Self::Pro => "pro",
            Self::Business => "business",
        }
    }

    /// Maximum number of monitors that are not heartbeats.
    pub const fn monitor_quota(self) -> i64 {
        match self {
            Self::Free => 10,
            Self::Pro => 50,
            Self::Business => 200,
        }
    }

    /// Maximum number of heartbeat monitors.
    pub const fn heartbeat_quota(self) -> i64 {
        match self {
            Self::Free => 10,
            Self::Pro => 50,
            Self::Business => 200,
        }
    }

    /// Maximum number of API keys that are not revoked.
    pub const fn api_key_quota(self) -> i64 {
        match self {
            Self::Free | Self::Pro => 20,
            Self::Business => 50,
        }
    }

    /// Requests per minute the tenant's API keys may make, together (docs/taut.md).
    pub const fn api_key_requests_per_minute(self) -> u64 {
        match self {
            Self::Free => 60,
            Self::Pro => 300,
            Self::Business => 600,
        }
    }

    /// Requests per minute the panel may make for the tenant, all members and tabs together:
    /// a monitor's page alone polls about 20 times a minute.
    pub const fn panel_requests_per_minute(self) -> u64 {
        match self {
            Self::Free => 600,
            Self::Pro => 1_200,
            Self::Business => 3_000,
        }
    }

    /// Every quota of the plan, as the panel shows it.
    pub const fn limits(self) -> PlanLimits {
        PlanLimits {
            monitors: self.monitor_quota(),
            heartbeats: self.heartbeat_quota(),
            api_keys: self.api_key_quota(),
            api_key_requests_per_minute: self.api_key_requests_per_minute(),
            panel_requests_per_minute: self.panel_requests_per_minute(),
        }
    }
}

/// The quotas of a plan, in the JSON of a tenant, so the panel never hard-codes them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct PlanLimits {
    /// Monitors that are not heartbeats.
    pub monitors: i64,
    /// Heartbeat monitors.
    pub heartbeats: i64,
    /// API keys that are not revoked.
    pub api_keys: i64,
    /// Requests per minute of all the tenant's API keys together.
    pub api_key_requests_per_minute: u64,
    /// Requests per minute of the panel, all members together.
    pub panel_requests_per_minute: u64,
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

/// A tenant seen by one of its members: the tenant, their role in it and what its plan allows.
#[derive(Debug, Clone, Serialize)]
pub struct TenantWithRole {
    /// The tenant.
    #[serde(flatten)]
    pub tenant: Tenant,
    /// The caller's role.
    pub role: Role,
    /// Quotas of the tenant's plan.
    pub limits: PlanLimits,
}

impl TenantWithRole {
    /// The tenant as `role` sees it, with the quotas of its plan.
    pub const fn new(tenant: Tenant, role: Role) -> Self {
        let limits = tenant.plan.limits();
        Self {
            tenant,
            role,
            limits,
        }
    }
}

/// Looks a tenant up by id, with the role of `user_id` in it; `None` unless they are a member.
/// Runs in a transaction of that tenant, so row-level security alone already keeps every
/// other tenant out.
pub async fn find_for_member(
    pool: &PgPool,
    tenant_id: Uuid,
    user_id: Uuid,
) -> Result<Option<TenantWithRole>, sqlx::Error> {
    let mut tx = taut_db::begin_tenant(pool, tenant_id).await?;
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
    .fetch_optional(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(row.map(|row| {
        TenantWithRole::new(
            Tenant {
                id: row.id,
                slug: row.slug,
                name: row.name,
                plan: row.plan,
                created_at: row.created_at,
            },
            row.role,
        )
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

/// Path prefixes an API key may reach: monitors (with their checks and metrics) and
/// incidents. Every other tenant route answers `403` to a key, so a new route stays closed to
/// keys until it is added here.
pub const API_KEY_ROUTES: &[&str] = &["/v1/monitors", "/v1/incidents"];

/// How often, at most, a key's `last_used_at` is written (the `UPDATE` in `touch` says the
/// same in SQL).
const LAST_USED_RESOLUTION_SECS: i64 = 60;

/// Whether an API key may reach `path` (see [`API_KEY_ROUTES`]).
pub fn api_key_may_reach(path: &str) -> bool {
    API_KEY_ROUTES.iter().any(|prefix| {
        path.strip_prefix(prefix)
            .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
    })
}

fn api_key_denied() -> AppError {
    AppError::Forbidden("api keys cannot manage the tenant".to_owned())
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
    /// A tenant API key: `read` only reads, `write` also manages monitors.
    ApiKey {
        /// Key id (not the key).
        key_id: Uuid,
        /// What the key may do.
        scope: Scope,
    },
}

impl Actor {
    /// Fails with `403` unless this actor may do `permission` here.
    pub fn require(&self, permission: Permission) -> Result<(), AppError> {
        match (self, permission) {
            (_, Permission::Read) | (Self::User { .. }, Permission::Monitors) => Ok(()),
            (Self::User { role, .. }, Permission::Channels | Permission::Tenant) => {
                if *role == Role::Owner {
                    Ok(())
                } else {
                    Err(AppError::Forbidden("owner role required".to_owned()))
                }
            }
            (Self::ApiKey { scope, .. }, Permission::Monitors) => match scope {
                Scope::Write => Ok(()),
                Scope::Read => Err(AppError::Forbidden("this api key is read-only".to_owned())),
            },
            (Self::ApiKey { .. }, Permission::Channels | Permission::Tenant) => {
                Err(api_key_denied())
            }
        }
    }

    /// Fails with `403 reauth_required` unless the user confirmed their password within the
    /// last five minutes (`POST /v1/auth/reauth` on the auth service). For the actions a
    /// stolen session must not be enough for: alert channels, API keys, deleting the tenant.
    /// An API key never passes: it cannot prove anything again.
    pub fn require_recent_auth(&self) -> Result<(), AppError> {
        let Self::User { auth_time, .. } = self else {
            return Err(api_key_denied());
        };
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

    /// The user behind the request; `403` for an API key, which has none.
    pub fn user_id(&self) -> Result<Uuid, AppError> {
        match self {
            Self::User { user_id, .. } => Ok(*user_id),
            Self::ApiKey { .. } => Err(api_key_denied()),
        }
    }
}

/// The signed-in user, from the bearer token alone. For routes that are not about one tenant
/// (listing or creating tenants); API keys get `401` here.
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
                    "api keys are not accepted on this route".to_owned(),
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
/// header and a membership joining them; or an API key, whose tenant is its own.
///
/// `401` without a valid token, `400` without a well-formed header, `403` with code
/// `not_a_member` when the user does not belong to that tenant (or it does not exist: the
/// answer never tells which). An API key answers `401` when unknown or revoked, `403` when
/// the header names another tenant, the route is not in [`API_KEY_ROUTES`] or a `read` key
/// tries anything but `GET`. On success the tenant id (and the key id) is recorded on the
/// request span and the request counts against the tenant's budget (API keys or panel, see
/// [`crate::ratelimit`]): `429` once it is spent.
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
        if let Some(Credential::ApiKey(key)) = bearer::credential(&parts.headers) {
            return from_api_key(parts, state, key).await;
        }
        let user = CurrentUser::from_request_parts(parts, state).await?;
        let id = tenant_header(parts)?
            .ok_or_else(|| AppError::BadRequest(format!("missing {TENANT_ID_HEADER} header")))?;
        let found = find_for_member(&state.pool, id, user.user_id)
            .await?
            .ok_or_else(|| AppError::Denied {
                code: "not_a_member",
                message: "you are not a member of this tenant".to_owned(),
            })?;
        tracing::Span::current().record("tenant_id", tracing::field::display(found.tenant.id));
        state
            .limiter
            .hit(
                found.tenant.id,
                Bucket::Panel,
                found.tenant.plan.panel_requests_per_minute(),
            )
            .await?;
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

/// The `X-Tenant-Id` header: `None` when absent, `400` when it is not a UUID.
fn tenant_header(parts: &Parts) -> Result<Option<Uuid>, AppError> {
    let Some(header) = parts.headers.get(TENANT_ID_HEADER) else {
        return Ok(None);
    };
    header
        .to_str()
        .ok()
        .and_then(|value| Uuid::parse_str(value.trim()).ok())
        .map(Some)
        .ok_or_else(|| AppError::BadRequest(format!("{TENANT_ID_HEADER} must be a tenant id")))
}

async fn from_api_key(
    parts: &Parts,
    state: &AppState,
    key: &str,
) -> Result<CurrentTenant, AppError> {
    let hash = api_key::lookup_hash(key)
        .ok_or_else(|| AppError::Unauthorized("invalid api key".to_owned()))?;
    // The key is what names the tenant: the lookup spans every tenant (owner rights).
    let mut tx = state.system_tx().await?;
    let row = sqlx::query!(
        r#"
        SELECT k.id AS key_id, k.scope, k.last_used_at,
               t.id, t.slug, t.name, t.plan AS "plan: Plan", t.created_at
        FROM api_keys k
        JOIN tenants t ON t.id = k.tenant_id
        WHERE k.key_hash = $1 AND k.revoked_at IS NULL
        "#,
        hash,
    )
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| AppError::Unauthorized("invalid or revoked api key".to_owned()))?;
    tx.commit().await?;
    let span = tracing::Span::current();
    span.record("tenant_id", tracing::field::display(row.id));
    span.record("api_key_id", tracing::field::display(row.key_id));

    if tenant_header(parts)?.is_some_and(|id| id != row.id) {
        return Err(AppError::Forbidden(
            "this api key belongs to another tenant".to_owned(),
        ));
    }
    if !api_key_may_reach(parts.uri.path()) {
        return Err(api_key_denied());
    }
    let scope: Scope = row.scope.parse().map_err(anyhow::Error::from)?;
    if scope == Scope::Read && !matches!(parts.method, Method::GET | Method::HEAD) {
        return Err(AppError::Forbidden("this api key is read-only".to_owned()));
    }
    state
        .limiter
        .hit(
            row.id,
            Bucket::ApiKey,
            row.plan.api_key_requests_per_minute(),
        )
        .await?;

    let due = row.last_used_at.is_none_or(|at| {
        Utc::now().signed_duration_since(at).num_seconds() >= LAST_USED_RESOLUTION_SECS
    });
    if due {
        touch(state.pool.clone(), row.id, row.key_id);
    }

    Ok(CurrentTenant {
        tenant: Tenant {
            id: row.id,
            slug: row.slug,
            name: row.name,
            plan: row.plan,
            created_at: row.created_at,
        },
        actor: Actor::ApiKey {
            key_id: row.key_id,
            scope,
        },
    })
}

/// Writes `last_used_at` in the background: the request never waits for it, and the
/// condition keeps concurrent requests from writing more than once a minute.
fn touch(pool: PgPool, tenant_id: Uuid, key_id: Uuid) {
    tokio::spawn(async move {
        let result = async {
            let mut tx = taut_db::begin_tenant(&pool, tenant_id).await?;
            sqlx::query!(
                r#"
                UPDATE api_keys SET last_used_at = now()
                WHERE tenant_id = $1 AND id = $2
                  AND (last_used_at IS NULL OR last_used_at < now() - interval '1 minute')
                "#,
                tenant_id,
                key_id,
            )
            .execute(&mut *tx)
            .await?;
            tx.commit().await
        }
        .await;
        if let Err(err) = result {
            tracing::warn!(api_key_id = %key_id, error = %err, "could not record api key use");
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn larger_plans_allow_more_and_keep_the_database_names() {
        for pair in Plan::ALL.windows(2) {
            let (smaller, larger) = (pair[0].limits(), pair[1].limits());
            assert!(larger.monitors > smaller.monitors, "{pair:?}");
            assert!(larger.heartbeats > smaller.heartbeats, "{pair:?}");
            assert!(larger.api_keys >= smaller.api_keys, "{pair:?}");
            assert!(larger.api_key_requests_per_minute > smaller.api_key_requests_per_minute);
            assert!(larger.panel_requests_per_minute > smaller.panel_requests_per_minute);
        }
        for plan in Plan::ALL {
            assert_eq!(serde_json::to_value(plan).unwrap(), plan.as_str());
        }
        // Free keeps the quotas of the prototype, so existing tenants see no change.
        assert_eq!(Plan::Free.limits().monitors, 10);
        assert_eq!(Plan::Free.limits().api_key_requests_per_minute, 60);
    }

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
    fn api_keys_read_or_manage_monitors_and_nothing_else() {
        let read = Actor::ApiKey {
            key_id: Uuid::nil(),
            scope: Scope::Read,
        };
        let write = Actor::ApiKey {
            key_id: Uuid::nil(),
            scope: Scope::Write,
        };
        assert!(read.require(Permission::Read).is_ok());
        assert!(read.require(Permission::Monitors).is_err());
        assert!(write.require(Permission::Read).is_ok());
        assert!(write.require(Permission::Monitors).is_ok());
        for key in [&read, &write] {
            assert!(key.require(Permission::Channels).is_err());
            assert!(key.require(Permission::Tenant).is_err());
            assert!(key.require_recent_auth().is_err());
            assert!(key.user_id().is_err());
        }
    }

    #[test]
    fn api_keys_reach_only_the_monitoring_routes() {
        for path in [
            "/v1/monitors",
            "/v1/monitors/0192/metrics",
            "/v1/incidents",
            "/v1/incidents/0192",
        ] {
            assert!(api_key_may_reach(path), "{path}");
        }
        for path in [
            "/v1/members",
            "/v1/api-keys",
            "/v1/alert-channels",
            "/v1/tenant",
            "/v1/monitorsx",
            "/v1/invitations",
        ] {
            assert!(!api_key_may_reach(path), "{path}");
        }
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

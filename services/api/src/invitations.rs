//! `/v1/invitations`: owners invite people to their tenant by email.
//!
//! An invitation carries a secret that travels only in the emailed link (`/invite/{token}` on
//! the panel); the table keeps its SHA-256. Anyone holding the link can read who is invited
//! and to what (`GET /v1/invitations/{token}`, no session needed), but only a signed-in user
//! whose email is the invited one can accept it. Inviting the same address again replaces the
//! pending invitation, so the newest link is the only one that works.

// Axum handlers receive their state by value even when they only borrow it.
#![allow(clippy::needless_pass_by_value)]

use axum::Router;
use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::{get, post};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgConnection;
use taut_http::{AppError, Json, Path, current_trace_id};
use taut_identity::random;
use taut_model::events::AccountEmail;
use uuid::Uuid;

use crate::AppState;
use crate::tenant::{CurrentTenant, CurrentUser, Permission, Plan, Role, Tenant, TenantWithRole};
use crate::validate;

/// Pending invitations a tenant may have at once.
const MAX_PENDING: i64 = 20;

/// How long a link works.
const TTL_DAYS: i64 = 7;

/// Routes under `/v1/invitations`. The second segment is an invitation id for owners
/// (`DELETE`) and the secret of the link for the invitee (`GET`, `accept`): axum needs one
/// name per position, and the handlers parse what they expect.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/v1/invitations", get(list).post(create))
        .route("/v1/invitations/{id}", get(preview).delete(revoke))
        .route("/v1/invitations/{id}/accept", post(accept))
}

/// A pending invitation, as owners see it.
#[derive(Debug, Clone, Serialize)]
pub struct Invitation {
    /// Identifier.
    pub id: Uuid,
    /// Invited address.
    pub email: String,
    /// Role offered.
    pub role: Role,
    /// Name of the owner who invited, if their account still exists.
    pub invited_by: Option<String>,
    /// When the link stops working.
    pub expires_at: DateTime<Utc>,
    /// When it was sent.
    pub created_at: DateTime<Utc>,
}

/// What the link shows before accepting: enough to decide, nothing else of the tenant.
#[derive(Debug, Clone, Serialize)]
pub struct InvitationPreview {
    /// Display name of the inviting tenant.
    pub tenant_name: String,
    /// Invited address; only that account can accept.
    pub email: String,
    /// Role offered.
    pub role: Role,
    /// When the link stops working.
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct NewInvitation {
    email: String,
    role: Role,
}

fn not_found() -> AppError {
    AppError::NotFound("invitation not found".to_owned())
}

/// Trimmed, lowercased address, the form `users.email` stores.
fn normalize_email(raw: &str) -> Result<String, AppError> {
    let email = raw.trim().to_lowercase();
    if email.chars().count() >= 3 && validate::is_email(&email) {
        Ok(email)
    } else {
        Err(AppError::BadRequest(
            "email is not a valid address".to_owned(),
        ))
    }
}

/// Locks the tenant row and checks that `email` may be invited: not a member yet, and room
/// for one more pending invitation once its previous one (expired or not) gives way.
async fn make_room(tx: &mut PgConnection, tenant_id: Uuid, email: &str) -> Result<(), AppError> {
    sqlx::query!("SELECT id FROM tenants WHERE id = $1 FOR UPDATE", tenant_id)
        .fetch_one(&mut *tx)
        .await?;
    let already_member = sqlx::query_scalar!(
        r#"
        SELECT EXISTS (
            SELECT 1 FROM memberships m JOIN users u ON u.id = m.user_id
            WHERE m.tenant_id = $1 AND u.email = $2
        ) AS "exists!"
        "#,
        tenant_id,
        email,
    )
    .fetch_one(&mut *tx)
    .await?;
    if already_member {
        return Err(AppError::Conflict(
            "that address is already a member of the tenant".to_owned(),
        ));
    }
    sqlx::query!(
        "DELETE FROM invitations WHERE tenant_id = $1 AND email = $2 AND accepted_at IS NULL",
        tenant_id,
        email,
    )
    .execute(&mut *tx)
    .await?;
    let pending = sqlx::query_scalar!(
        r#"
        SELECT count(*) AS "count!" FROM invitations
        WHERE tenant_id = $1 AND accepted_at IS NULL AND expires_at > now()
        "#,
        tenant_id,
    )
    .fetch_one(&mut *tx)
    .await?;
    if pending >= MAX_PENDING {
        return Err(AppError::Forbidden(format!(
            "a tenant can have at most {MAX_PENDING} pending invitations"
        )));
    }
    Ok(())
}

async fn create(
    State(state): State<AppState>,
    CurrentTenant { tenant, actor }: CurrentTenant,
    Json(input): Json<NewInvitation>,
) -> Result<(StatusCode, Json<Invitation>), AppError> {
    actor.require(Permission::Tenant)?;
    let email = normalize_email(&input.email)?;

    let mut tx = state.pool.begin().await?;
    make_room(&mut tx, tenant.id, &email).await?;
    let inviter = sqlx::query_scalar!("SELECT name FROM users WHERE id = $1", actor.user_id())
        .fetch_one(&mut *tx)
        .await?;

    let id = Uuid::now_v7();
    let token = random::opaque_token();
    let expires_at = Utc::now() + Duration::days(TTL_DAYS);
    let created_at = sqlx::query_scalar!(
        r#"
        INSERT INTO invitations (id, tenant_id, email, role, token_hash, invited_by, expires_at)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        RETURNING created_at
        "#,
        id,
        tenant.id,
        email,
        input.role.as_str(),
        random::sha256_hex(&token),
        actor.user_id(),
        expires_at,
    )
    .fetch_one(&mut *tx)
    .await?;

    // Published before the commit: if the bus refuses, nothing is stored and the owner
    // retries.
    let email_event = AccountEmail::MemberInvited {
        trace_id: current_trace_id().unwrap_or_else(|| ulid::Ulid::generate().to_string()),
        to: email.clone(),
        tenant_id: tenant.id,
        tenant_name: tenant.name.clone(),
        invited_by: inviter.clone(),
        role: input.role.as_str().to_owned(),
        link: format!(
            "{}/invite/{token}",
            state.settings.app.panel_url.trim_end_matches('/')
        ),
        expires_at,
    };
    state
        .bus
        .publish_once(
            email_event.subject(),
            email_event.trace_id(),
            &id.to_string(),
            &email_event,
        )
        .await
        .map_err(|err| {
            tracing::warn!(error = %err, "invitation email not published");
            AppError::Unavailable("the invitation email could not be queued; retry".to_owned())
        })?;
    tx.commit().await?;
    tracing::info!(invitation_id = %id, role = input.role.as_str(), "member invited");

    Ok((
        StatusCode::CREATED,
        Json(Invitation {
            id,
            email,
            role: input.role,
            invited_by: Some(inviter),
            expires_at,
            created_at,
        }),
    ))
}

async fn list(
    State(state): State<AppState>,
    CurrentTenant { tenant, actor }: CurrentTenant,
) -> Result<Json<Vec<Invitation>>, AppError> {
    actor.require(Permission::Tenant)?;
    let invitations = sqlx::query_as!(
        Invitation,
        r#"
        SELECT i.id, i.email, i.role AS "role: Role", u.name AS "invited_by?", i.expires_at,
               i.created_at
        FROM invitations i
        LEFT JOIN users u ON u.id = i.invited_by
        WHERE i.tenant_id = $1 AND i.accepted_at IS NULL
        ORDER BY i.created_at DESC
        "#,
        tenant.id,
    )
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(invitations))
}

async fn revoke(
    State(state): State<AppState>,
    CurrentTenant { tenant, actor }: CurrentTenant,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    actor.require(Permission::Tenant)?;
    let deleted = sqlx::query!(
        "DELETE FROM invitations WHERE tenant_id = $1 AND id = $2 AND accepted_at IS NULL",
        tenant.id,
        id,
    )
    .execute(&state.pool)
    .await?;
    if deleted.rows_affected() == 0 {
        return Err(not_found());
    }
    tracing::info!(invitation_id = %id, "invitation revoked");
    Ok(StatusCode::NO_CONTENT)
}

/// `404` for an unknown or used link, `400` for an expired one.
fn check_usable(
    accepted_at: Option<DateTime<Utc>>,
    expires_at: DateTime<Utc>,
) -> Result<(), AppError> {
    if accepted_at.is_some() {
        return Err(not_found());
    }
    if expires_at <= Utc::now() {
        return Err(AppError::BadRequest("the invitation expired".to_owned()));
    }
    Ok(())
}

async fn preview(
    State(state): State<AppState>,
    Path(token): Path<String>,
) -> Result<Json<InvitationPreview>, AppError> {
    let row = sqlx::query!(
        r#"
        SELECT t.name AS tenant_name, i.email, i.role AS "role: Role", i.expires_at, i.accepted_at
        FROM invitations i
        JOIN tenants t ON t.id = i.tenant_id
        WHERE i.token_hash = $1
        "#,
        random::sha256_hex(&token),
    )
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(not_found)?;
    check_usable(row.accepted_at, row.expires_at)?;
    Ok(Json(InvitationPreview {
        tenant_name: row.tenant_name,
        email: row.email,
        role: row.role,
        expires_at: row.expires_at,
    }))
}

async fn accept(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(token): Path<String>,
) -> Result<Json<TenantWithRole>, AppError> {
    let mut tx = state.pool.begin().await?;
    let invitation = sqlx::query!(
        r#"
        SELECT id, tenant_id, email, role AS "role: Role", expires_at, accepted_at
        FROM invitations
        WHERE token_hash = $1
        FOR UPDATE
        "#,
        random::sha256_hex(&token),
    )
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(not_found)?;
    check_usable(invitation.accepted_at, invitation.expires_at)?;

    let email = sqlx::query_scalar!("SELECT email FROM users WHERE id = $1", user.user_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| AppError::Unauthorized("the account no longer exists".to_owned()))?;
    if email != invitation.email {
        return Err(AppError::Forbidden(
            "this invitation is for another email address".to_owned(),
        ));
    }

    let joined = sqlx::query!(
        "INSERT INTO memberships (user_id, tenant_id, role) VALUES ($1, $2, $3)
         ON CONFLICT DO NOTHING",
        user.user_id,
        invitation.tenant_id,
        invitation.role.as_str(),
    )
    .execute(&mut *tx)
    .await?;
    if joined.rows_affected() == 0 {
        return Err(AppError::Conflict(
            "you are already a member of this tenant".to_owned(),
        ));
    }
    sqlx::query!(
        "UPDATE invitations SET accepted_at = now() WHERE id = $1",
        invitation.id
    )
    .execute(&mut *tx)
    .await?;
    let tenant = sqlx::query_as!(
        Tenant,
        r#"SELECT id, slug, name, plan AS "plan: Plan", created_at FROM tenants WHERE id = $1"#,
        invitation.tenant_id,
    )
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;
    tracing::info!(
        invitation_id = %invitation.id,
        tenant_id = %tenant.id,
        user_id = %user.user_id,
        "invitation accepted"
    );
    Ok(Json(TenantWithRole {
        tenant,
        role: invitation.role,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emails_are_trimmed_and_lowercased() {
        assert_eq!(
            normalize_email("  Bea@Example.COM ").unwrap(),
            "bea@example.com"
        );
        for bad in [
            "",
            "bea",
            "bea@",
            "@example.com",
            "bea@example",
            "b ea@example.com",
        ] {
            assert!(normalize_email(bad).is_err(), "{bad:?} should be rejected");
        }
    }

    #[test]
    fn used_or_expired_links_do_not_work() {
        let later = Utc::now() + Duration::days(1);
        assert!(check_usable(None, later).is_ok());
        assert_eq!(
            check_usable(Some(Utc::now()), later).unwrap_err().code(),
            "not_found"
        );
        let expired = check_usable(None, Utc::now() - Duration::seconds(1)).unwrap_err();
        assert_eq!(expired.code(), "bad_request");
    }
}

//! Registration and email verification.
//!
//! Registering answers `202` whether or not the address exists: an existing account gets an
//! email saying so instead, so the response never tells who has an account.

use axum::extract::State;
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use taut_http::{AppError, ClientIp, Json};
use taut_model::events::AccountEmail;

use crate::ratelimit::{EMAILS_PER_ADDRESS, REQUESTS_PER_IP, ip_key};
use crate::tokens::{self, TokenKind};
use crate::users::{self, User};
use crate::{AppState, mail, password};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RegisterInput {
    name: String,
    email: String,
    password: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct Pending {
    status: &'static str,
}

/// `POST /v1/auth/register`.
pub(crate) async fn register(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    Json(input): Json<RegisterInput>,
) -> Result<(StatusCode, Json<Pending>), AppError> {
    let email = users::normalize_email(&input.email)?;
    let name = users::normalize_name(&input.name)?;
    password::check_policy(&input.password, &email)?;
    state
        .limiter
        .check(&ip_key("register", ip), REQUESTS_PER_IP)
        .await?;
    state
        .limiter
        .hit(&ip_key("register", ip), REQUESTS_PER_IP)
        .await?;

    if let Some(existing) = users::find_by_email(&state.pool, &email).await? {
        if existing.email_verified_at.is_some() {
            already_registered(&state, &existing).await?;
        } else {
            send_verification(&state, &existing).await?;
        }
        metrics::counter!("auth_registrations_total", "result" => "existing").increment(1);
        return Ok((
            StatusCode::ACCEPTED,
            Json(Pending {
                status: "pending_verification",
            }),
        ));
    }

    let hash = password::hash(&input.password).await?;
    let user = match users::insert(&state.pool, &email, &name, &hash).await? {
        Some(user) => user,
        // Registered between the lookup and the insert: treat as existing.
        None => users::find_by_email(&state.pool, &email)
            .await?
            .ok_or_else(|| AppError::Internal(anyhow::anyhow!("user vanished after insert")))?,
    };
    tracing::info!(user_id = %user.id, "user registered");
    metrics::counter!("auth_registrations_total", "result" => "created").increment(1);
    send_verification(&state, &user).await?;
    Ok((
        StatusCode::ACCEPTED,
        Json(Pending {
            status: "pending_verification",
        }),
    ))
}

/// Emails a fresh verification link, unless the address got too many emails already.
pub(crate) async fn send_verification(state: &AppState, user: &User) -> Result<(), AppError> {
    let key = format!("rl:email:{}", user.email);
    if !state.limiter.try_hit(&key, EMAILS_PER_ADDRESS).await? {
        tracing::info!(user_id = %user.id, "verification email skipped: address limit");
        return Ok(());
    }
    let issued = tokens::issue(&state.pool, TokenKind::VerifyEmail, user.id).await?;
    let email = AccountEmail::VerificationRequested {
        trace_id: mail::trace_id(),
        to: user.email.clone(),
        name: user.name.clone(),
        link: mail::link(
            &state.settings.app.panel_url,
            "verify-email",
            &issued.secret,
        ),
        expires_at: issued.expires_at,
    };
    mail::publish(&state.bus, &issued.id.to_string(), &email).await
}

async fn already_registered(state: &AppState, user: &User) -> Result<(), AppError> {
    let key = format!("rl:email:{}", user.email);
    if !state.limiter.try_hit(&key, EMAILS_PER_ADDRESS).await? {
        return Ok(());
    }
    let panel = &state.settings.app.panel_url;
    let email = AccountEmail::AlreadyRegistered {
        trace_id: mail::trace_id(),
        to: user.email.clone(),
        name: user.name.clone(),
        login_link: mail::page(panel, "login"),
        reset_link: mail::page(panel, "forgot-password"),
    };
    let message_id = format!("already-registered:{}:{}", user.id, ulid::Ulid::generate());
    mail::publish(&state.bus, &message_id, &email).await
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TokenInput {
    token: String,
}

/// `POST /v1/auth/verify-email`.
pub(crate) async fn verify_email(
    State(state): State<AppState>,
    Json(input): Json<TokenInput>,
) -> Result<StatusCode, AppError> {
    let user_id = tokens::consume(&state.pool, TokenKind::VerifyEmail, &input.token)
        .await?
        .ok_or_else(|| AppError::BadRequest("invalid or expired token".to_owned()))?;
    users::mark_verified(&state.pool, user_id).await?;
    tracing::info!(%user_id, "email verified");
    metrics::counter!("auth_verifications_total").increment(1);
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct EmailInput {
    email: String,
}

/// `POST /v1/auth/resend-verification`. Always `202`.
pub(crate) async fn resend_verification(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    Json(input): Json<EmailInput>,
) -> Result<StatusCode, AppError> {
    let email = users::normalize_email(&input.email)?;
    state
        .limiter
        .check(&ip_key("register", ip), REQUESTS_PER_IP)
        .await?;
    if let Some(user) = users::find_by_email(&state.pool, &email).await?
        && user.email_verified_at.is_none()
    {
        send_verification(&state, &user).await?;
    }
    Ok(StatusCode::ACCEPTED)
}

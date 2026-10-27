//! Password recovery by email.

use axum::extract::State;
use axum::http::StatusCode;
use serde::Deserialize;
use taut_http::{AppError, ClientIp, Json};
use taut_model::events::AccountEmail;

use crate::ratelimit::{EMAILS_PER_ADDRESS, REQUESTS_PER_IP, ip_key};
use crate::tokens::{self, TokenKind};
use crate::{AppState, mail, password, users};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ForgotInput {
    email: String,
    /// Turnstile token of the form; required when the captcha is on.
    #[serde(default)]
    captcha_token: Option<String>,
}

/// `POST /v1/auth/forgot-password`: always `202`, emails a link when the account exists.
pub(crate) async fn forgot_password(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    Json(input): Json<ForgotInput>,
) -> Result<StatusCode, AppError> {
    let email = users::normalize_email(&input.email)?;
    let ip_key = ip_key("reset", ip);
    state.limiter.check(&ip_key, REQUESTS_PER_IP).await?;
    state.limiter.hit(&ip_key, REQUESTS_PER_IP).await?;
    state
        .captcha
        .verify(input.captcha_token.as_deref(), ip)
        .await?;
    let Some(user) = users::find_by_email(&state.pool, &email).await? else {
        tracing::info!("password reset requested for an unknown address");
        return Ok(StatusCode::ACCEPTED);
    };
    let key = format!("rl:email:{}", user.email);
    if !state.limiter.try_hit(&key, EMAILS_PER_ADDRESS).await? {
        return Ok(StatusCode::ACCEPTED);
    }
    let issued = tokens::issue(&state.pool, TokenKind::ResetPassword, user.id).await?;
    let email = AccountEmail::PasswordResetRequested {
        trace_id: mail::trace_id(),
        to: user.email.clone(),
        name: user.name.clone(),
        link: mail::link(
            &state.settings.app.panel_url,
            "reset-password",
            &issued.secret,
        ),
        expires_at: issued.expires_at,
    };
    mail::publish(&state.bus, &issued.id.to_string(), &email).await?;
    tracing::info!(user_id = %user.id, "password reset email published");
    Ok(StatusCode::ACCEPTED)
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ResetInput {
    token: String,
    password: String,
}

/// `POST /v1/auth/reset-password`: sets the password, confirms the address and ends every
/// session.
pub(crate) async fn reset_password(
    State(state): State<AppState>,
    Json(input): Json<ResetInput>,
) -> Result<StatusCode, AppError> {
    let invalid = || AppError::BadRequest("invalid or expired token".to_owned());
    // The policy is checked before the token is spent, so a rejected password does not burn
    // the link.
    let user_id = tokens::peek(&state.pool, TokenKind::ResetPassword, &input.token)
        .await?
        .ok_or_else(invalid)?;
    let user = users::find_by_id(&state.pool, user_id)
        .await?
        .ok_or_else(invalid)?;
    password::check_policy(&input.password, &user.email)?;
    let hash = password::hash(&input.password).await?;
    tokens::consume(&state.pool, TokenKind::ResetPassword, &input.token)
        .await?
        .ok_or_else(invalid)?;
    users::set_password(&state.pool, user.id, &hash, true).await?;
    tracing::info!(user_id = %user.id, "password reset, sessions revoked");
    metrics::counter!("auth_password_resets_total").increment(1);
    Ok(StatusCode::NO_CONTENT)
}

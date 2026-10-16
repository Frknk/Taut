//! Refresh, logout, `me` and password change.

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum_extra::extract::CookieJar;
use serde::Deserialize;
use taut_http::{AppError, Json};

use crate::current_user::CurrentUser;
use crate::ratelimit::LOGIN_PER_ACCOUNT;
use crate::routes::{SessionResponse, respond, start_session};
use crate::sessions::{COOKIE_NAME, Rotated, Session, SessionStore};
use crate::users::PublicUser;
use crate::{AppState, password, redis_error, users};

/// `POST /v1/auth/refresh`: rotates the refresh cookie and mints a new access token. Any
/// rejection also clears the cookie, so the panel goes straight to the login page.
pub(crate) async fn refresh(State(state): State<AppState>, jar: CookieJar) -> Response {
    match refresh_session(&state, &jar).await {
        Ok((session, token, user)) => {
            metrics::counter!("auth_refresh_total", "result" => "ok").increment(1);
            respond(&state, jar, &session, token, &user).into_response()
        }
        Err(err @ AppError::Unauthorized(_)) => {
            metrics::counter!("auth_refresh_total", "result" => "rejected").increment(1);
            (
                jar.add(SessionStore::clear_cookie(state.secure_cookies)),
                err,
            )
                .into_response()
        }
        Err(err) => err.into_response(),
    }
}

/// Rotates the cookie's session; `Unauthorized` means the session is over.
async fn refresh_session(
    state: &AppState,
    jar: &CookieJar,
) -> Result<(Session, String, users::User), AppError> {
    let cookie = jar
        .get(COOKIE_NAME)
        .map(|c| c.value().to_owned())
        .ok_or_else(|| AppError::Unauthorized("no session".to_owned()))?;
    let (session, token) = match state
        .sessions
        .rotate(&cookie)
        .await
        .map_err(|err| redis_error(&err))?
    {
        Rotated::Ok { session, token } => (session, token),
        Rotated::Reused => {
            tracing::warn!("refresh token reuse detected, session revoked");
            return Err(AppError::Unauthorized("session revoked".to_owned()));
        }
        Rotated::Invalid => return Err(AppError::Unauthorized("session expired".to_owned())),
    };
    let user = users::find_by_id(&state.pool, session.user_id)
        .await?
        .filter(|user| user.token_version == session.token_version);
    let Some(user) = user else {
        state
            .sessions
            .revoke_sid(session.sid)
            .await
            .map_err(|err| redis_error(&err))?;
        return Err(AppError::Unauthorized("session ended".to_owned()));
    };
    Ok((session, token, user))
}

/// `POST /v1/auth/logout`: ends the session of the cookie, if any. Idempotent.
pub(crate) async fn logout(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<(CookieJar, StatusCode), AppError> {
    if let Some(cookie) = jar.get(COOKIE_NAME) {
        state
            .sessions
            .revoke(cookie.value())
            .await
            .map_err(|err| redis_error(&err))?;
    }
    Ok((
        jar.add(SessionStore::clear_cookie(state.secure_cookies)),
        StatusCode::NO_CONTENT,
    ))
}

/// `GET /v1/auth/me`.
pub(crate) async fn me(current: CurrentUser) -> Json<PublicUser> {
    Json(current.user.public())
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PasswordChange {
    current_password: String,
    new_password: String,
}

/// `PUT /v1/auth/password`: every other session ends; this one restarts, so the caller stays
/// signed in and gets a fresh cookie. A sensitive action: needs a recent authentication
/// besides the current password.
pub(crate) async fn change_password(
    State(state): State<AppState>,
    current: CurrentUser,
    jar: CookieJar,
    Json(input): Json<PasswordChange>,
) -> Result<(CookieJar, Json<SessionResponse>), AppError> {
    current.require_recent_auth()?;
    let user = current.user;
    let key = format!("rl:login:acct:{}", user.email);
    state.limiter.check(&key, LOGIN_PER_ACCOUNT).await?;
    if !password::verify(&input.current_password, Some(&user.password_hash)).await? {
        state.limiter.hit(&key, LOGIN_PER_ACCOUNT).await?;
        return Err(AppError::Unauthorized(
            "current password is wrong".to_owned(),
        ));
    }
    password::check_policy(&input.new_password, &user.email)?;
    let hash = password::hash(&input.new_password).await?;
    let token_version = users::set_password(&state.pool, user.id, &hash, false).await?;
    state
        .sessions
        .revoke_sid(current.claims.sid)
        .await
        .map_err(|err| redis_error(&err))?;
    tracing::info!(user_id = %user.id, "password changed, sessions revoked");
    let user = users::User {
        password_hash: hash,
        token_version,
        ..user
    };
    start_session(&state, jar, &user, current.claims.mfa).await
}

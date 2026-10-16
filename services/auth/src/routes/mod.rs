//! `/v1/auth/*` and `/.well-known/jwks.json`.

// Axum handlers receive their state by value even when they only borrow it.
#![allow(clippy::needless_pass_by_value)]

pub mod jwks;
pub mod login;
pub mod mfa;
pub mod register;
pub mod reset;
pub mod session;

use axum::Router;
use axum::routing::{get, post};
use axum_extra::extract::CookieJar;
use chrono::Utc;
use serde::Serialize;
use taut_http::{AppError, Json};
use taut_identity::{Claims, TokenKind};
use ulid::Ulid;
use uuid::Uuid;

use crate::AppState;
use crate::redis_error;
use crate::sessions::Session;
use crate::users::{PublicUser, User};

/// Seconds an MFA token (between the password and the second factor) lives.
pub const MFA_TOKEN_TTL_SECS: u64 = crate::mfa::MFA_TOKEN_TTL_SECS;

/// All the routes of the service.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/v1/auth/register", post(register::register))
        .route("/v1/auth/verify-email", post(register::verify_email))
        .route(
            "/v1/auth/resend-verification",
            post(register::resend_verification),
        )
        .route("/v1/auth/login", post(login::login))
        .route("/v1/auth/mfa", get(mfa::status).post(mfa::complete))
        .route("/v1/auth/mfa/enroll", post(mfa::enroll))
        .route("/v1/auth/mfa/confirm", post(mfa::confirm))
        .route("/v1/auth/mfa/disable", post(mfa::disable))
        .route("/v1/auth/mfa/backup-codes", post(mfa::backup_codes))
        .route("/v1/auth/reauth", post(mfa::reauth))
        .route("/v1/auth/refresh", post(session::refresh))
        .route("/v1/auth/logout", post(session::logout))
        .route("/v1/auth/me", get(session::me))
        .route(
            "/v1/auth/password",
            axum::routing::put(session::change_password),
        )
        .route("/v1/auth/forgot-password", post(reset::forgot_password))
        .route("/v1/auth/reset-password", post(reset::reset_password))
        .route("/.well-known/jwks.json", get(jwks::jwks))
}

/// What a login or refresh answers: the access token and who the user is. The refresh token
/// goes in the cookie, never in the body.
#[derive(Debug, Serialize)]
pub struct SessionResponse {
    /// The access JWT.
    pub access_token: String,
    /// Always `Bearer`.
    pub token_type: &'static str,
    /// Seconds until the access token expires.
    pub expires_in: u64,
    /// The user.
    pub user: PublicUser,
}

/// Signs an access token for `session`.
pub fn access_token(state: &AppState, session: &Session) -> Result<(String, u64), AppError> {
    let ttl = state.settings.auth.access_ttl_secs;
    let now = Utc::now().timestamp();
    let claims = Claims {
        iss: state.settings.auth.issuer.clone(),
        sub: session.user_id,
        jti: Ulid::generate().to_string(),
        sid: session.sid,
        iat: now,
        exp: now + i64::try_from(ttl).unwrap_or(i64::MAX),
        auth_time: session.auth_time,
        mfa: session.mfa,
        ver: session.token_version,
        kind: TokenKind::Access,
    };
    let token = state
        .keys
        .sign(&claims)
        .map_err(|err| AppError::Internal(err.into()))?;
    Ok((token, ttl))
}

/// Signs the token that bridges a correct password and the second factor. It names the user
/// and nothing else: no session exists until the code is right.
pub fn mfa_token(state: &AppState, user: &User) -> Result<String, AppError> {
    let now = Utc::now().timestamp();
    let claims = Claims {
        iss: state.settings.auth.issuer.clone(),
        sub: user.id,
        jti: Ulid::generate().to_string(),
        sid: Uuid::nil(),
        iat: now,
        exp: now + i64::try_from(MFA_TOKEN_TTL_SECS).unwrap_or(i64::MAX),
        auth_time: now,
        mfa: false,
        ver: user.token_version,
        kind: TokenKind::Mfa,
    };
    state
        .keys
        .sign(&claims)
        .map_err(|err| AppError::Internal(err.into()))
}

/// Starts a session for `user` (a fresh login): stores it, signs the access token and sets
/// the refresh cookie. `mfa` says whether the login passed the second factor.
pub async fn start_session(
    state: &AppState,
    jar: CookieJar,
    user: &User,
    mfa: bool,
) -> Result<(CookieJar, Json<SessionResponse>), AppError> {
    let session = Session {
        sid: Uuid::now_v7(),
        user_id: user.id,
        token_version: user.token_version,
        mfa,
        auth_time: Utc::now().timestamp(),
    };
    let refresh = state
        .sessions
        .create(&session)
        .await
        .map_err(|err| redis_error(&err))?;
    respond(state, jar, &session, refresh, user)
}

/// Answers with the access token for `session` and `refresh` in the cookie.
pub fn respond(
    state: &AppState,
    jar: CookieJar,
    session: &Session,
    refresh: String,
    user: &User,
) -> Result<(CookieJar, Json<SessionResponse>), AppError> {
    let (access, expires_in) = access_token(state, session)?;
    let jar = jar.add(state.sessions.cookie(refresh, state.secure_cookies));
    Ok((
        jar,
        Json(SessionResponse {
            access_token: access,
            token_type: "Bearer",
            expires_in,
            user: user.public(),
        }),
    ))
}

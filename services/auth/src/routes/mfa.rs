//! Second factor: enrollment, the second step of the login and re-authentication.
//!
//! `POST /v1/auth/mfa/enroll` → `POST /v1/auth/mfa/confirm {code}` turns 2FA on and hands out
//! the backup codes once. With 2FA on, `POST /v1/auth/login` answers `mfa_required` with a
//! short-lived MFA token and `POST /v1/auth/mfa {mfa_token, code | backup_code}` completes the
//! session. `POST /v1/auth/reauth {password, code?}` refreshes `auth_time` for the sensitive
//! actions (step-up); `mfa/disable` and `mfa/backup-codes` require it.

use axum::extract::State;
use axum::http::StatusCode;
use axum_extra::extract::CookieJar;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use taut_http::{AppError, Json};
use taut_identity::TokenKind;

use crate::current_user::CurrentUser;
use crate::mfa::{self, Checked, Factor};
use crate::ratelimit::{LOGIN_PER_ACCOUNT, MFA_PER_ACCOUNT};
use crate::routes::{SessionResponse, access_token, start_session};
use crate::users::User;
use crate::{AppState, password, redis_error, users};

/// What `enroll` answers: the secret for manual entry and the URI behind the QR code.
#[derive(Debug, Serialize)]
pub(crate) struct Enrollment {
    /// Base32 secret, for apps that cannot scan.
    secret: String,
    /// `otpauth://totp/taut:{email}?secret=…&issuer=taut`, rendered as a QR by the panel.
    otpauth_uri: String,
}

/// Backup codes in their printable form. Shown once; only their hashes remain.
#[derive(Debug, Serialize)]
pub(crate) struct BackupCodes {
    backup_codes: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ConfirmInput {
    code: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct DisableInput {
    password: String,
    code: Option<String>,
    backup_code: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CompleteInput {
    mfa_token: String,
    code: Option<String>,
    backup_code: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReauthInput {
    password: String,
    code: Option<String>,
    backup_code: Option<String>,
}

fn invalid_code() -> AppError {
    AppError::Unauthorized("invalid code".to_owned())
}

fn already_enabled() -> AppError {
    AppError::Conflict("two-factor authentication is already enabled".to_owned())
}

fn not_enabled() -> AppError {
    AppError::Conflict("two-factor authentication is not enabled".to_owned())
}

/// `POST /v1/auth/mfa/enroll`: a fresh secret, kept in Redis until `confirm`.
pub(crate) async fn enroll(
    State(state): State<AppState>,
    current: CurrentUser,
) -> Result<Json<Enrollment>, AppError> {
    let user = current.user;
    if user.mfa_enabled() {
        return Err(already_enabled());
    }
    let secret = mfa::generate_secret();
    let totp = mfa::totp(secret.as_bytes(), &user.email)?;
    let otpauth_uri = totp
        .to_url()
        .map_err(|err| AppError::Internal(anyhow::anyhow!("otpauth uri: {err}")))?;
    state.mfa.set_pending(user.id, &secret.to_base32()).await?;
    tracing::info!(user_id = %user.id, "mfa enrollment started");
    Ok(Json(Enrollment {
        secret: secret.to_base32(),
        otpauth_uri,
    }))
}

/// `POST /v1/auth/mfa/confirm {code}`: the code proves the app has the secret; 2FA turns on
/// and the backup codes come back, once.
pub(crate) async fn confirm(
    State(state): State<AppState>,
    current: CurrentUser,
    Json(input): Json<ConfirmInput>,
) -> Result<Json<BackupCodes>, AppError> {
    let user = current.user;
    if user.mfa_enabled() {
        return Err(already_enabled());
    }
    let pending =
        state.mfa.pending(user.id).await?.ok_or_else(|| {
            AppError::BadRequest("no enrollment in progress; start again".to_owned())
        })?;
    let secret = totp_rs::Secret::try_from_base32(&pending)
        .map_err(|_| AppError::Internal(anyhow::anyhow!("pending secret is not base32")))?;
    let factor = Factor::from_input(Some(&input.code), None)?;
    let Factor::Totp(code) = factor else {
        return Err(invalid_code());
    };
    check_totp(&state, &user, secret.as_bytes(), &code).await?;

    let encrypted = state.totp.encrypt(secret.as_bytes());
    users::enable_totp(&state.pool, user.id, &encrypted).await?;
    let codes = mfa::new_backup_codes();
    mfa::store_backup_codes(&state.pool, user.id, &codes).await?;
    state.mfa.clear_pending(user.id).await?;
    // This session just passed the second factor; the next refresh says so in the token.
    state
        .sessions
        .touch(current.claims.sid, |session| session.mfa = true)
        .await
        .map_err(|err| redis_error(&err))?;
    metrics::counter!("auth_mfa_total", "result" => "enabled").increment(1);
    tracing::info!(user_id = %user.id, "mfa enabled");
    Ok(Json(BackupCodes {
        backup_codes: codes,
    }))
}

/// `POST /v1/auth/mfa/disable {password, code | backup_code}`. Needs a recent
/// authentication besides the password and a code: a stolen session alone cannot weaken the
/// account.
pub(crate) async fn disable(
    State(state): State<AppState>,
    current: CurrentUser,
    Json(input): Json<DisableInput>,
) -> Result<StatusCode, AppError> {
    current.require_recent_auth()?;
    let user = current.user;
    if !user.mfa_enabled() {
        return Err(not_enabled());
    }
    check_password(&state, &user, &input.password).await?;
    let factor = Factor::from_input(input.code.as_deref(), input.backup_code.as_deref())?;
    verify_factor(&state, &user, &factor).await?;
    users::disable_totp(&state.pool, user.id).await?;
    state
        .sessions
        .touch(current.claims.sid, |session| session.mfa = false)
        .await
        .map_err(|err| redis_error(&err))?;
    metrics::counter!("auth_mfa_total", "result" => "disabled").increment(1);
    tracing::info!(user_id = %user.id, "mfa disabled");
    Ok(StatusCode::NO_CONTENT)
}

/// `POST /v1/auth/mfa/backup-codes`: eight new codes; the old ones stop working.
pub(crate) async fn backup_codes(
    State(state): State<AppState>,
    current: CurrentUser,
) -> Result<Json<BackupCodes>, AppError> {
    current.require_recent_auth()?;
    let user = current.user;
    if !user.mfa_enabled() {
        return Err(not_enabled());
    }
    let codes = mfa::new_backup_codes();
    mfa::store_backup_codes(&state.pool, user.id, &codes).await?;
    tracing::info!(user_id = %user.id, "mfa backup codes regenerated");
    Ok(Json(BackupCodes {
        backup_codes: codes,
    }))
}

/// What `GET /v1/auth/mfa` answers: the state of the second factor.
#[derive(Debug, Serialize)]
pub(crate) struct MfaStatus {
    enabled: bool,
    enabled_at: Option<chrono::DateTime<Utc>>,
    backup_codes_left: i64,
}

/// `GET /v1/auth/mfa`: whether 2FA is on and how many backup codes remain.
pub(crate) async fn status(
    State(state): State<AppState>,
    current: CurrentUser,
) -> Result<Json<MfaStatus>, AppError> {
    let user = current.user;
    let left = if user.mfa_enabled() {
        mfa::backup_codes_left(&state.pool, user.id).await?
    } else {
        0
    };
    Ok(Json(MfaStatus {
        enabled: user.mfa_enabled(),
        enabled_at: user.totp_enabled_at,
        backup_codes_left: left,
    }))
}

/// `POST /v1/auth/mfa {mfa_token, code | backup_code}`: second step of the login. The MFA
/// token (from `login`) names the user; the code completes the session.
pub(crate) async fn complete(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(input): Json<CompleteInput>,
) -> Result<(CookieJar, Json<SessionResponse>), AppError> {
    let claims = state
        .verifier
        .verify(&input.mfa_token)
        .await
        .map_err(|err| AppError::Unauthorized(err.to_string()))?;
    if claims.kind != TokenKind::Mfa {
        return Err(AppError::Unauthorized("not an mfa token".to_owned()));
    }
    let spent = || AppError::Unauthorized("mfa token already used; login again".to_owned());
    if state.mfa.is_spent(&claims.jti).await? {
        return Err(spent());
    }
    let user = users::find_by_id(&state.pool, claims.sub)
        .await?
        .filter(|user| user.token_version == claims.ver && user.mfa_enabled())
        .ok_or_else(|| AppError::Unauthorized("login again".to_owned()))?;
    let factor = Factor::from_input(input.code.as_deref(), input.backup_code.as_deref())?;
    verify_factor(&state, &user, &factor).await?;
    if !state.mfa.spend_token(&claims.jti).await? {
        return Err(spent());
    }
    metrics::counter!("auth_logins_total", "result" => "ok").increment(1);
    tracing::info!(user_id = %user.id, "login succeeded with second factor");
    start_session(&state, jar, &user, true).await
}

/// `POST /v1/auth/reauth {password, code?}`: the user proves who they are again; the session
/// gets a new `auth_time` and the caller a token that passes the sensitive routes for five
/// minutes. The refresh cookie does not change.
pub(crate) async fn reauth(
    State(state): State<AppState>,
    current: CurrentUser,
    Json(input): Json<ReauthInput>,
) -> Result<Json<SessionResponse>, AppError> {
    let user = current.user;
    check_password(&state, &user, &input.password).await?;
    if user.mfa_enabled() {
        let factor = Factor::from_input(input.code.as_deref(), input.backup_code.as_deref())?;
        verify_factor(&state, &user, &factor).await?;
    }
    let now = Utc::now().timestamp();
    let mfa = user.mfa_enabled();
    let session = state
        .sessions
        .touch(current.claims.sid, |session| {
            session.auth_time = now;
            session.mfa = session.mfa || mfa;
        })
        .await
        .map_err(|err| redis_error(&err))?
        .ok_or_else(|| AppError::Unauthorized("session ended".to_owned()))?;
    let (access, expires_in) = access_token(&state, &session)?;
    metrics::counter!("auth_reauth_total", "result" => "ok").increment(1);
    tracing::info!(user_id = %user.id, "re-authenticated");
    Ok(Json(SessionResponse {
        access_token: access,
        token_type: "Bearer",
        expires_in,
        user: user.public(),
    }))
}

/// Verifies `password` for `user` under the account login limit.
async fn check_password(state: &AppState, user: &User, password: &str) -> Result<(), AppError> {
    let key = format!("rl:login:acct:{}", user.email);
    state.limiter.check(&key, LOGIN_PER_ACCOUNT).await?;
    if !password::verify(password, Some(&user.password_hash)).await? {
        state.limiter.hit(&key, LOGIN_PER_ACCOUNT).await?;
        metrics::counter!("auth_reauth_total", "result" => "failed").increment(1);
        return Err(AppError::Unauthorized("wrong password".to_owned()));
    }
    Ok(())
}

/// Verifies `factor` for `user`, whose 2FA is on. Wrong codes count against `rl:mfa`.
async fn verify_factor(state: &AppState, user: &User, factor: &Factor) -> Result<(), AppError> {
    match factor {
        Factor::Totp(code) => {
            let stored = user.totp_secret.as_deref().ok_or_else(not_enabled)?;
            let secret = state.totp.decrypt(stored).map_err(|err| {
                tracing::error!(user_id = %user.id, "stored totp secret does not decrypt");
                AppError::Internal(err.into())
            })?;
            check_totp(state, user, &secret, code).await
        }
        Factor::Backup(code) => {
            let key = format!("rl:mfa:{}", user.id);
            state.limiter.check(&key, MFA_PER_ACCOUNT).await?;
            if mfa::consume_backup_code(&state.pool, user.id, code).await? {
                state.limiter.reset(&key).await?;
                metrics::counter!("auth_mfa_total", "result" => "backup_code").increment(1);
                tracing::info!(user_id = %user.id, "backup code used");
                Ok(())
            } else {
                state.limiter.hit(&key, MFA_PER_ACCOUNT).await?;
                metrics::counter!("auth_mfa_total", "result" => "failed").increment(1);
                Err(invalid_code())
            }
        }
    }
}

/// Checks a TOTP `code` against `secret` with the replay and rate limits of `user`.
async fn check_totp(
    state: &AppState,
    user: &User,
    secret: &[u8],
    code: &str,
) -> Result<(), AppError> {
    let key = format!("rl:mfa:{}", user.id);
    state.limiter.check(&key, MFA_PER_ACCOUNT).await?;
    let totp = mfa::totp(secret, &user.email)?;
    match state.mfa.check_totp(user.id, &totp, code).await? {
        Checked::Ok => {
            state.limiter.reset(&key).await?;
            metrics::counter!("auth_mfa_total", "result" => "ok").increment(1);
            Ok(())
        }
        Checked::Replayed => {
            state.limiter.hit(&key, MFA_PER_ACCOUNT).await?;
            metrics::counter!("auth_mfa_total", "result" => "replay").increment(1);
            tracing::warn!(user_id = %user.id, "totp code replayed");
            Err(AppError::Unauthorized(
                "code already used; wait for the next one".to_owned(),
            ))
        }
        Checked::Wrong => {
            state.limiter.hit(&key, MFA_PER_ACCOUNT).await?;
            metrics::counter!("auth_mfa_total", "result" => "failed").increment(1);
            tracing::info!(user_id = %user.id, "wrong totp code");
            Err(invalid_code())
        }
    }
}

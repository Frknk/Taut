//! `POST /v1/auth/login`.

use axum::extract::State;
use axum::response::{IntoResponse, Response};
use axum_extra::extract::CookieJar;
use serde::{Deserialize, Serialize};
use taut_http::{AppError, ClientIp, Json};

use crate::ratelimit::{LOGIN_PER_ACCOUNT, LOGIN_PER_IP, ip_key};
use crate::routes::{SessionResponse, mfa_token, start_session};
use crate::{AppState, password, users};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct LoginInput {
    email: String,
    password: String,
}

/// Answer of a login whose password was right but whose account has a second factor: the
/// panel asks for the code and sends it with `mfa_token` to `POST /v1/auth/mfa`.
#[derive(Debug, Serialize)]
pub(crate) struct MfaRequired {
    /// Always `mfa_required`.
    status: &'static str,
    /// Short-lived token naming the user; not an access token.
    mfa_token: String,
}

/// Either a session or the request for a second factor; both `200`.
pub(crate) enum LoginOutcome {
    Session(CookieJar, Json<SessionResponse>),
    MfaRequired(Json<MfaRequired>),
}

impl IntoResponse for LoginOutcome {
    fn into_response(self) -> Response {
        match self {
            Self::Session(jar, body) => (jar, body).into_response(),
            Self::MfaRequired(body) => body.into_response(),
        }
    }
}

fn invalid_credentials() -> AppError {
    AppError::Unauthorized("invalid email or password".to_owned())
}

/// Checks the password and starts a session, or asks for the second factor when the account
/// has one. A wrong password counts against the account and the address; an unverified
/// account is told so only after the right password.
pub(crate) async fn login(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    jar: CookieJar,
    Json(input): Json<LoginInput>,
) -> Result<LoginOutcome, AppError> {
    let email = users::normalize_email(&input.email).map_err(|_| invalid_credentials())?;
    let account_key = format!("rl:login:acct:{email}");
    let ip_key = ip_key("login", ip);
    state.limiter.check(&account_key, LOGIN_PER_ACCOUNT).await?;
    state.limiter.check(&ip_key, LOGIN_PER_IP).await?;

    let user = users::find_by_email(&state.pool, &email).await?;
    let matched = password::verify(
        &input.password,
        user.as_ref().map(|user| user.password_hash.as_str()),
    )
    .await?;
    let Some(user) = user.filter(|_| matched) else {
        state.limiter.hit(&account_key, LOGIN_PER_ACCOUNT).await?;
        state.limiter.hit(&ip_key, LOGIN_PER_IP).await?;
        metrics::counter!("auth_logins_total", "result" => "failed").increment(1);
        tracing::info!("login failed");
        return Err(invalid_credentials());
    };
    if user.email_verified_at.is_none() {
        metrics::counter!("auth_logins_total", "result" => "unverified").increment(1);
        return Err(AppError::Denied {
            code: "email_unverified",
            message: "confirm your email address before signing in".to_owned(),
        });
    }
    state.limiter.reset(&account_key).await?;
    if user.mfa_enabled() {
        metrics::counter!("auth_logins_total", "result" => "mfa_required").increment(1);
        tracing::info!(user_id = %user.id, "password accepted, second factor required");
        return Ok(LoginOutcome::MfaRequired(Json(MfaRequired {
            status: "mfa_required",
            mfa_token: mfa_token(&state, &user)?,
        })));
    }
    metrics::counter!("auth_logins_total", "result" => "ok").increment(1);
    tracing::info!(user_id = %user.id, "login succeeded");
    let (jar, body) = start_session(&state, jar, &user, false).await?;
    Ok(LoginOutcome::Session(jar, body))
}

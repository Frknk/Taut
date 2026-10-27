//! Cloudflare Turnstile: proof that a person, not a script, filled the registration, login and
//! password recovery forms.
//!
//! The panel renders the widget and sends its token as `captcha_token`; the handler calls
//! [`Captcha::verify`] after its cheap checks and before any expensive work (Argon2, the
//! database, emails). Each token is valid for five minutes and only once: Cloudflare rejects a
//! replay. With no secret configured the captcha is off, which only development and tests
//! allow (see [`crate::captcha`]).

use std::net::IpAddr;
use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use taut_http::AppError;

/// Longest token Cloudflare issues; anything longer is not one of theirs.
const MAX_TOKEN_LEN: usize = 2048;
/// Time allowed for `siteverify` to answer.
const VERIFY_TIMEOUT: Duration = Duration::from_secs(5);

/// Verifies Turnstile tokens; cheap to clone.
#[derive(Clone)]
pub struct Captcha(Option<Arc<Inner>>);

struct Inner {
    client: reqwest::Client,
    secret: String,
    url: String,
}

#[derive(Serialize)]
struct VerifyRequest<'a> {
    secret: &'a str,
    response: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    remoteip: Option<String>,
}

#[derive(Deserialize)]
struct VerifyResponse {
    success: bool,
    #[serde(rename = "error-codes", default)]
    error_codes: Vec<String>,
}

impl Captcha {
    /// Verifies against `url` (Turnstile's `siteverify`) with the widget's `secret`.
    pub fn new(secret: &str, url: &str) -> anyhow::Result<Self> {
        taut_net::install_crypto_provider();
        let client = reqwest::Client::builder().timeout(VERIFY_TIMEOUT).build()?;
        Ok(Self(Some(Arc::new(Inner {
            client,
            secret: secret.trim().to_owned(),
            url: url.to_owned(),
        }))))
    }

    /// A captcha that accepts every request, with or without a token.
    #[must_use]
    pub fn disabled() -> Self {
        Self(None)
    }

    /// Whether requests need a token.
    #[must_use]
    pub fn is_enabled(&self) -> bool {
        self.0.is_some()
    }

    /// Accepts the request when the captcha is off or Cloudflare vouches for `token`.
    ///
    /// # Errors
    ///
    /// `403 captcha_failed` when the token is missing, malformed, expired, already used or
    /// rejected; `503` when Cloudflare cannot be reached.
    pub async fn verify(&self, token: Option<&str>, ip: Option<IpAddr>) -> Result<(), AppError> {
        let Some(inner) = &self.0 else {
            return Ok(());
        };
        let token = token.map(str::trim).unwrap_or_default();
        if token.is_empty() || token.len() > MAX_TOKEN_LEN {
            metrics::counter!("auth_captcha_total", "result" => "missing").increment(1);
            return Err(failed());
        }
        let request = VerifyRequest {
            secret: &inner.secret,
            response: token,
            remoteip: ip.map(|ip| ip.to_string()),
        };
        let answer = inner
            .client
            .post(&inner.url)
            .json(&request)
            .send()
            .await
            .and_then(reqwest::Response::error_for_status);
        let answer = match answer {
            Ok(response) => response.json::<VerifyResponse>().await,
            Err(err) => Err(err),
        };
        match answer {
            Ok(VerifyResponse { success: true, .. }) => {
                metrics::counter!("auth_captcha_total", "result" => "passed").increment(1);
                Ok(())
            }
            Ok(VerifyResponse { error_codes, .. }) => {
                metrics::counter!("auth_captcha_total", "result" => "failed").increment(1);
                tracing::info!(?error_codes, "captcha rejected");
                Err(failed())
            }
            Err(err) => {
                metrics::counter!("auth_captcha_total", "result" => "error").increment(1);
                tracing::warn!(error = %err, "captcha verification unavailable");
                Err(AppError::Unavailable(
                    "captcha verification unavailable".to_owned(),
                ))
            }
        }
    }
}

fn failed() -> AppError {
    AppError::Denied {
        code: "captcha_failed",
        message: "captcha verification failed".to_owned(),
    }
}

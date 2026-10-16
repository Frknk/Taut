//! Verifying tokens against the auth service's JWKs.

use std::collections::HashMap;
use std::sync::RwLock;
#[cfg(feature = "fetch")]
use std::time::{Duration, Instant};

use jsonwebtoken::{Algorithm, DecodingKey, Validation};

use crate::claims::{Claims, TokenKind};
use crate::keys::Jwks;

/// Tokens that expired this many seconds ago still pass, to absorb clock drift between
/// services.
const LEEWAY_SECS: u64 = 30;
/// Minimum time between two JWKS fetches caused by unknown key ids, so a flood of bad tokens
/// cannot hammer the auth service.
#[cfg(feature = "fetch")]
const REFETCH_INTERVAL: Duration = Duration::from_secs(10);

/// Why a token was rejected.
#[derive(Debug, thiserror::Error)]
pub enum VerifyError {
    /// Not a JWT, wrong algorithm or bad signature.
    #[error("invalid token")]
    Invalid,
    /// Signed by a key this verifier does not know, even after refreshing.
    #[error("unknown signing key")]
    UnknownKey,
    /// Past its expiration.
    #[error("token expired")]
    Expired,
    /// Valid, but not the kind the caller accepts (an MFA token used as access token).
    #[error("wrong token kind")]
    WrongKind,
    /// The JWKS could not be fetched.
    #[error("jwks fetch failed: {0}")]
    Fetch(String),
}

#[cfg(feature = "fetch")]
struct Remote {
    url: String,
    client: reqwest::Client,
    last_fetch: std::sync::Mutex<Option<Instant>>,
}

/// Verifies `EdDSA` tokens with a set of public keys, static or fetched from a JWKS URL.
pub struct Verifier {
    validation: Validation,
    keys: RwLock<HashMap<String, DecodingKey>>,
    #[cfg(feature = "fetch")]
    remote: Option<Remote>,
}

impl std::fmt::Debug for Verifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Verifier")
            .field("keys", &self.key_count())
            .finish()
    }
}

impl Verifier {
    fn new(issuer: &str) -> Self {
        let mut validation = Validation::new(Algorithm::EdDSA);
        validation.set_issuer(&[issuer]);
        validation.set_required_spec_claims(&["exp", "iss", "sub"]);
        validation.leeway = LEEWAY_SECS;
        Self {
            validation,
            keys: RwLock::new(HashMap::new()),
            #[cfg(feature = "fetch")]
            remote: None,
        }
    }

    /// Verifier over a fixed JWK set issued by `issuer`.
    pub fn from_jwks(jwks: &Jwks, issuer: &str) -> Self {
        let verifier = Self::new(issuer);
        verifier.replace_keys(jwks);
        verifier
    }

    /// Verifier that fetches the JWK set from `url` (the auth service) with [`Verifier::refresh`]
    /// and again on its own when a token names a key it does not have.
    #[cfg(feature = "fetch")]
    pub fn remote(url: &str, issuer: &str, client: reqwest::Client) -> Self {
        let mut verifier = Self::new(issuer);
        verifier.remote = Some(Remote {
            url: url.to_owned(),
            client,
            last_fetch: std::sync::Mutex::new(None),
        });
        verifier
    }

    /// Number of keys known right now; `/ready` wants at least one.
    pub fn key_count(&self) -> usize {
        self.keys.read().map_or(0, |keys| keys.len())
    }

    fn replace_keys(&self, jwks: &Jwks) {
        let keys = jwks
            .keys
            .iter()
            .filter_map(|jwk| {
                let kid = jwk.common.key_id.clone()?;
                let key = DecodingKey::from_jwk(jwk).ok()?;
                Some((kid, key))
            })
            .collect();
        if let Ok(mut current) = self.keys.write() {
            *current = keys;
        }
    }

    /// Fetches the JWK set now. Called at startup and by `/ready` recovery; a failure keeps
    /// the keys already loaded.
    #[cfg(feature = "fetch")]
    pub async fn refresh(&self) -> Result<(), VerifyError> {
        let Some(remote) = &self.remote else {
            return Ok(());
        };
        if let Ok(mut last) = remote.last_fetch.lock() {
            *last = Some(Instant::now());
        }
        let jwks: Jwks = remote
            .client
            .get(&remote.url)
            .send()
            .await
            .and_then(reqwest::Response::error_for_status)
            .map_err(|err| VerifyError::Fetch(err.without_url().to_string()))?
            .json()
            .await
            .map_err(|err| VerifyError::Fetch(err.without_url().to_string()))?;
        self.replace_keys(&jwks);
        tracing::info!(keys = self.key_count(), "jwks loaded");
        Ok(())
    }

    #[cfg(feature = "fetch")]
    fn may_refetch(&self) -> bool {
        self.remote.as_ref().is_some_and(|remote| {
            remote
                .last_fetch
                .lock()
                .is_ok_and(|last| last.is_none_or(|at| at.elapsed() >= REFETCH_INTERVAL))
        })
    }

    fn decode(&self, token: &str, kid: &str) -> Option<Result<Claims, VerifyError>> {
        let keys = self.keys.read().ok()?;
        let key = keys.get(kid)?;
        Some(
            jsonwebtoken::decode::<Claims>(token, key, &self.validation)
                .map(|data| data.claims)
                .map_err(|err| match err.kind() {
                    jsonwebtoken::errors::ErrorKind::ExpiredSignature => VerifyError::Expired,
                    _ => VerifyError::Invalid,
                }),
        )
    }

    /// Verifies signature, issuer and expiration and returns the claims, whatever their kind.
    // Awaits the JWKS refetch only with the `fetch` feature.
    #[allow(clippy::unused_async, clippy::unused_async_trait_impl)]
    pub async fn verify(&self, token: &str) -> Result<Claims, VerifyError> {
        let header = jsonwebtoken::decode_header(token).map_err(|_| VerifyError::Invalid)?;
        if header.alg != Algorithm::EdDSA {
            return Err(VerifyError::Invalid);
        }
        let kid = header.kid.ok_or(VerifyError::UnknownKey)?;
        if let Some(result) = self.decode(token, &kid) {
            return result;
        }
        #[cfg(feature = "fetch")]
        if self.may_refetch() {
            tracing::info!(kid, "token signed by an unknown key, refreshing jwks");
            self.refresh().await?;
            if let Some(result) = self.decode(token, &kid) {
                return result;
            }
        }
        Err(VerifyError::UnknownKey)
    }

    /// Like [`Verifier::verify`], but only accepts access tokens.
    pub async fn verify_access(&self, token: &str) -> Result<Claims, VerifyError> {
        let claims = self.verify(token).await?;
        if claims.kind != TokenKind::Access {
            return Err(VerifyError::WrongKind);
        }
        Ok(claims)
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use uuid::Uuid;

    use super::*;
    use crate::keys::KeyPair;

    fn claims(kind: TokenKind, ttl_secs: i64) -> Claims {
        let now = Utc::now().timestamp();
        Claims {
            iss: "taut-auth".to_owned(),
            sub: Uuid::now_v7(),
            jti: "01J".to_owned(),
            sid: Uuid::now_v7(),
            iat: now,
            exp: now + ttl_secs,
            auth_time: now,
            mfa: false,
            ver: 1,
            kind,
        }
    }

    #[tokio::test]
    async fn signed_tokens_verify_and_keep_their_claims() {
        let key = KeyPair::generate();
        let verifier = Verifier::from_jwks(&key.jwks(), "taut-auth");
        let claims = claims(TokenKind::Access, 60);
        let token = key.sign(&claims).unwrap();
        assert_eq!(verifier.verify_access(&token).await.unwrap(), claims);
    }

    #[tokio::test]
    async fn expired_tokens_are_rejected_after_the_leeway() {
        let key = KeyPair::generate();
        let verifier = Verifier::from_jwks(&key.jwks(), "taut-auth");
        let token = key.sign(&claims(TokenKind::Access, -120)).unwrap();
        assert!(matches!(
            verifier.verify(&token).await,
            Err(VerifyError::Expired)
        ));
        let token = key.sign(&claims(TokenKind::Access, -5)).unwrap();
        assert!(verifier.verify(&token).await.is_ok(), "within the leeway");
    }

    #[tokio::test]
    async fn wrong_issuer_key_or_kind_is_rejected() {
        let key = KeyPair::generate();
        let other = KeyPair::generate();
        let verifier = Verifier::from_jwks(&key.jwks(), "taut-auth");

        let mut foreign = claims(TokenKind::Access, 60);
        foreign.iss = "someone-else".to_owned();
        assert!(matches!(
            verifier.verify(&key.sign(&foreign).unwrap()).await,
            Err(VerifyError::Invalid)
        ));

        let token = other.sign(&claims(TokenKind::Access, 60)).unwrap();
        assert!(matches!(
            verifier.verify(&token).await,
            Err(VerifyError::UnknownKey)
        ));

        let token = key.sign(&claims(TokenKind::Mfa, 60)).unwrap();
        assert!(verifier.verify(&token).await.is_ok());
        assert!(matches!(
            verifier.verify_access(&token).await,
            Err(VerifyError::WrongKind)
        ));
    }

    #[tokio::test]
    async fn tampered_and_unsigned_tokens_are_invalid() {
        let key = KeyPair::generate();
        let verifier = Verifier::from_jwks(&key.jwks(), "taut-auth");
        let token = key.sign(&claims(TokenKind::Access, 60)).unwrap();
        let mut parts: Vec<&str> = token.split('.').collect();
        let altered = format!("{}x", parts[1]);
        parts[1] = &altered;
        assert!(matches!(
            verifier.verify(&parts.join(".")).await,
            Err(VerifyError::Invalid)
        ));
        assert!(matches!(
            verifier.verify("not.a.jwt").await,
            Err(VerifyError::Invalid)
        ));
        // `alg: none` with the right kid must not pass either.
        let header = base64::Engine::encode(
            &base64::engine::general_purpose::URL_SAFE_NO_PAD,
            format!(r#"{{"alg":"none","kid":"{}"}}"#, key.kid()),
        );
        let forged = format!("{header}.{}.", parts[1]);
        assert!(matches!(
            verifier.verify(&forged).await,
            Err(VerifyError::Invalid)
        ));
    }
}

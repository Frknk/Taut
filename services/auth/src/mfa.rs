//! Second factor: TOTP (RFC 6238) secrets encrypted at rest, backup codes and the Redis keys
//! that keep a code from being accepted twice.
//!
//! - The TOTP secret lives in `users.totp_secret` encrypted with AES-256-GCM under
//!   `TAUT_AUTH__TOTP_KEY` ([`TotpCipher`]); a database dump alone does not give it away.
//! - Backup codes (`xxxx-xxxx`, eight per user) are stored as SHA-256 in `mfa_backup_codes`
//!   and each works once.
//! - Redis: `mfa:pending:{user_id}` holds the secret of an enrollment not yet confirmed
//!   (10 min); `totp:used:{user_id}:{step}` marks a code already accepted (90 s, longer than
//!   the ±1 step window); `mfa:used:{jti}` marks an MFA token already spent.

use std::time::{SystemTime, UNIX_EPOCH};

use aes_gcm::aead::{Aead, Nonce};
use aes_gcm::{Aes256Gcm, KeyInit};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use redis::aio::ConnectionManager;
use sqlx::PgPool;
use taut_http::AppError;
use taut_identity::random::{sha256_hex, token_bytes};
use totp_rs::{Builder, Secret, Totp};
use uuid::Uuid;

use crate::redis_error;

/// `issuer` of the otpauth URI: what the authenticator app shows above the code.
pub const ISSUER: &str = "taut";
/// Backup codes handed out per enrollment.
pub const BACKUP_CODE_COUNT: usize = 8;
/// Seconds an enrollment waits for its confirmation code.
pub const PENDING_TTL_SECS: u64 = 600;
/// Seconds an accepted TOTP step stays blocked: three 30 s steps, the whole ±1 window.
pub const REPLAY_TTL_SECS: u64 = 90;
/// Seconds an MFA token lives (and stays marked as spent).
pub const MFA_TOKEN_TTL_SECS: u64 = 300;

/// Letters and digits of a backup code: lowercase, without the ones that read alike
/// (`0/o`, `1/l/i`).
const CODE_ALPHABET: &[u8] = b"abcdefghjkmnpqrstuvwxyz23456789";
/// Key used in development and tests when `TAUT_AUTH__TOTP_KEY` is empty. Not a secret.
const DEV_KEY: &[u8; 32] = b"taut-development-totp-key-000000";

/// Encrypts and decrypts TOTP secrets for storage.
#[derive(Clone)]
pub struct TotpCipher {
    cipher: Aes256Gcm,
}

impl std::fmt::Debug for TotpCipher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("TotpCipher")
    }
}

/// A stored secret that does not decrypt: wrong key, or a tampered row.
#[derive(Debug, thiserror::Error)]
#[error("stored totp secret does not decrypt")]
pub struct DecryptError;

impl TotpCipher {
    /// Cipher under `key`.
    pub fn new(key: &[u8; 32]) -> Self {
        Self {
            cipher: Aes256Gcm::new(key.into()),
        }
    }

    /// The fixed development key; the service warns when it falls back to it.
    pub fn dev() -> Self {
        Self::new(DEV_KEY)
    }

    /// `base64(nonce || ciphertext)` of `secret`, with a fresh random nonce.
    pub fn encrypt(&self, secret: &[u8]) -> String {
        let bytes = token_bytes();
        let nonce = Nonce::<Aes256Gcm>::try_from(&bytes[..12]).expect("12 bytes make a nonce");
        let ciphertext = self
            .cipher
            .encrypt(&nonce, secret)
            .expect("aes-gcm encrypts any input");
        let mut out = nonce.to_vec();
        out.extend_from_slice(&ciphertext);
        STANDARD.encode(out)
    }

    /// The secret back from what [`TotpCipher::encrypt`] stored.
    pub fn decrypt(&self, stored: &str) -> Result<Vec<u8>, DecryptError> {
        let bytes = STANDARD.decode(stored.trim()).map_err(|_| DecryptError)?;
        if bytes.len() < 12 {
            return Err(DecryptError);
        }
        let (nonce, ciphertext) = bytes.split_at(12);
        let nonce = Nonce::<Aes256Gcm>::try_from(nonce).map_err(|_| DecryptError)?;
        self.cipher
            .decrypt(&nonce, ciphertext)
            .map_err(|_| DecryptError)
    }
}

/// A fresh 160-bit secret, as RFC 4226 recommends.
pub fn generate_secret() -> Secret {
    Secret::generate()
}

/// The TOTP generator of `secret` for `email`: SHA-1, six digits, 30 s steps, ±1 step of
/// tolerance. The defaults every authenticator app understands.
pub fn totp(secret: &[u8], email: &str) -> Result<Totp, AppError> {
    Builder::new()
        .with_secret(secret.to_vec())
        .with_issuer(Some(ISSUER))
        .with_account_name(email)
        .build()
        .map_err(|err| AppError::Internal(anyhow::anyhow!("totp: {err}")))
}

/// Unix seconds now.
pub fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default()
}

/// Whether `code` looks like a TOTP code once spaces are removed: exactly six digits.
pub fn looks_like_totp(code: &str) -> bool {
    let digits: String = code.chars().filter(|c| !c.is_whitespace()).collect();
    digits.len() == 6 && digits.bytes().all(|b| b.is_ascii_digit())
}

/// `code` without spaces, for the TOTP check.
pub fn normalize_totp(code: &str) -> String {
    code.chars().filter(|c| !c.is_whitespace()).collect()
}

/// Eight new backup codes in their printable form (`xxxx-xxxx`).
pub fn new_backup_codes() -> Vec<String> {
    (0..BACKUP_CODE_COUNT)
        .map(|_| {
            let bytes = token_bytes();
            let chars: String = bytes[..8]
                .iter()
                .map(|b| CODE_ALPHABET[usize::from(*b) % CODE_ALPHABET.len()] as char)
                .collect();
            format!("{}-{}", &chars[..4], &chars[4..])
        })
        .collect()
}

/// The canonical `xxxx-xxxx` form of what the user typed (case, spaces and the dash do not
/// matter), or `None` when it cannot be a backup code.
pub fn normalize_backup_code(input: &str) -> Option<String> {
    let chars: String = input
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '-')
        .map(|c| c.to_ascii_lowercase())
        .collect();
    if chars.len() != 8 || !chars.bytes().all(|b| CODE_ALPHABET.contains(&b)) {
        return None;
    }
    Some(format!("{}-{}", &chars[..4], &chars[4..]))
}

/// The second factor a request presents.
#[derive(Debug)]
pub enum Factor {
    /// A code from the authenticator app.
    Totp(String),
    /// One of the backup codes.
    Backup(String),
}

impl Factor {
    /// Reads the `code` / `backup_code` pair of a request body; exactly one must be present.
    pub fn from_input(code: Option<&str>, backup_code: Option<&str>) -> Result<Self, AppError> {
        match (code, backup_code) {
            (Some(code), None) if looks_like_totp(code) => Ok(Self::Totp(normalize_totp(code))),
            (Some(_), None) => Err(AppError::BadRequest(
                "code must be the six digits of the authenticator app".to_owned(),
            )),
            (None, Some(backup)) => {
                normalize_backup_code(backup)
                    .map(Self::Backup)
                    .ok_or_else(|| {
                        AppError::BadRequest("backup_code must look like xxxx-xxxx".to_owned())
                    })
            }
            (None, None) => Err(AppError::BadRequest(
                "code or backup_code is required".to_owned(),
            )),
            (Some(_), Some(_)) => Err(AppError::BadRequest(
                "send either code or backup_code, not both".to_owned(),
            )),
        }
    }
}

/// Outcome of checking a code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Checked {
    /// Right, and not seen before.
    Ok,
    /// Right, but already accepted: a replay.
    Replayed,
    /// Wrong.
    Wrong,
}

/// The Redis side: pending enrollments and the replay marks.
#[derive(Clone)]
pub struct MfaStore {
    redis: ConnectionManager,
}

impl MfaStore {
    /// Store over `redis`.
    pub fn new(redis: ConnectionManager) -> Self {
        Self { redis }
    }

    /// Keeps the base32 secret of an enrollment until it is confirmed or expires.
    pub async fn set_pending(&self, user_id: Uuid, secret_base32: &str) -> Result<(), AppError> {
        let mut redis = self.redis.clone();
        redis::cmd("SET")
            .arg(format!("mfa:pending:{user_id}"))
            .arg(secret_base32)
            .arg("EX")
            .arg(PENDING_TTL_SECS)
            .exec_async(&mut redis)
            .await
            .map_err(|err| redis_error(&err))
    }

    /// The pending secret, if the enrollment is still open.
    pub async fn pending(&self, user_id: Uuid) -> Result<Option<String>, AppError> {
        let mut redis = self.redis.clone();
        redis::cmd("GET")
            .arg(format!("mfa:pending:{user_id}"))
            .query_async(&mut redis)
            .await
            .map_err(|err| redis_error(&err))
    }

    /// Forgets the pending enrollment.
    pub async fn clear_pending(&self, user_id: Uuid) -> Result<(), AppError> {
        let mut redis = self.redis.clone();
        redis::cmd("DEL")
            .arg(format!("mfa:pending:{user_id}"))
            .exec_async(&mut redis)
            .await
            .map_err(|err| redis_error(&err))
    }

    /// Checks `code` against `totp` now and, when it is right, marks its step as used.
    pub async fn check_totp(
        &self,
        user_id: Uuid,
        totp: &Totp,
        code: &str,
    ) -> Result<Checked, AppError> {
        let Some(step) = totp.check(code, now_secs()) else {
            return Ok(Checked::Wrong);
        };
        let fresh = self
            .claim(&format!("totp:used:{user_id}:{step}"), REPLAY_TTL_SECS)
            .await?;
        Ok(if fresh {
            Checked::Ok
        } else {
            Checked::Replayed
        })
    }

    /// Whether MFA token `jti` was already spent.
    pub async fn is_spent(&self, jti: &str) -> Result<bool, AppError> {
        let mut redis = self.redis.clone();
        redis::cmd("EXISTS")
            .arg(format!("mfa:used:{jti}"))
            .query_async(&mut redis)
            .await
            .map_err(|err| redis_error(&err))
    }

    /// Marks MFA token `jti` as spent; `false` when it already was.
    pub async fn spend_token(&self, jti: &str) -> Result<bool, AppError> {
        self.claim(&format!("mfa:used:{jti}"), MFA_TOKEN_TTL_SECS)
            .await
    }

    /// `SET NX`: `true` the first time `key` is claimed within `ttl_secs`.
    async fn claim(&self, key: &str, ttl_secs: u64) -> Result<bool, AppError> {
        let mut redis = self.redis.clone();
        let set: Option<String> = redis::cmd("SET")
            .arg(key)
            .arg("1")
            .arg("NX")
            .arg("EX")
            .arg(ttl_secs)
            .query_async(&mut redis)
            .await
            .map_err(|err| redis_error(&err))?;
        Ok(set.is_some())
    }
}

/// Replaces the user's backup codes with `codes` (their printable form) and returns nothing:
/// the caller shows them once.
pub async fn store_backup_codes(
    pool: &PgPool,
    user_id: Uuid,
    codes: &[String],
) -> Result<(), sqlx::Error> {
    let hashes: Vec<String> = codes.iter().map(|code| sha256_hex(code)).collect();
    let mut tx = pool.begin().await?;
    sqlx::query!("DELETE FROM mfa_backup_codes WHERE user_id = $1", user_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query!(
        "INSERT INTO mfa_backup_codes (user_id, code_hash)
         SELECT $1, unnest($2::text[])",
        user_id,
        &hashes,
    )
    .execute(&mut *tx)
    .await?;
    tx.commit().await
}

/// Spends backup `code` (canonical form) of `user_id`; `false` when it is unknown or used.
pub async fn consume_backup_code(
    pool: &PgPool,
    user_id: Uuid,
    code: &str,
) -> Result<bool, sqlx::Error> {
    let updated = sqlx::query!(
        "UPDATE mfa_backup_codes SET used_at = now()
         WHERE user_id = $1 AND code_hash = $2 AND used_at IS NULL",
        user_id,
        sha256_hex(code),
    )
    .execute(pool)
    .await?;
    Ok(updated.rows_affected() == 1)
}

/// Backup codes of `user_id` not yet used.
pub async fn backup_codes_left(pool: &PgPool, user_id: Uuid) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT count(*) AS "count!" FROM mfa_backup_codes
           WHERE user_id = $1 AND used_at IS NULL"#,
        user_id,
    )
    .fetch_one(pool)
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secrets_round_trip_and_tampering_is_detected() {
        let cipher = TotpCipher::new(&[7u8; 32]);
        let secret = generate_secret();
        let stored = cipher.encrypt(secret.as_bytes());
        assert_ne!(
            stored,
            cipher.encrypt(secret.as_bytes()),
            "a fresh nonce every time"
        );
        assert_eq!(cipher.decrypt(&stored).unwrap(), secret.as_bytes());

        let other = TotpCipher::new(&[8u8; 32]);
        assert!(other.decrypt(&stored).is_err(), "another key fails");
        let mut tampered = STANDARD.decode(&stored).unwrap();
        tampered[20] ^= 1;
        assert!(cipher.decrypt(&STANDARD.encode(tampered)).is_err());
        assert!(cipher.decrypt("short").is_err());
        assert!(cipher.decrypt("not base64!").is_err());
    }

    #[test]
    fn totp_uri_names_the_issuer_and_the_account() {
        let secret = generate_secret();
        let totp = totp(secret.as_bytes(), "ana@example.com").unwrap();
        let uri = totp.to_url().unwrap();
        assert!(
            uri.starts_with("otpauth://totp/taut:ana%40example.com?"),
            "{uri}"
        );
        assert!(
            uri.contains(&format!("secret={}", secret.to_base32())),
            "{uri}"
        );
        assert!(uri.contains("issuer=taut"), "{uri}");
        let now = now_secs();
        let code = totp.generate(now).to_string();
        assert_eq!(totp.check(&code, now), Some(now / 30));
        assert_eq!(totp.check("000000", now).is_some(), code == "000000");
    }

    #[test]
    fn backup_codes_have_the_printable_form_and_normalize() {
        let codes = new_backup_codes();
        assert_eq!(codes.len(), BACKUP_CODE_COUNT);
        for code in &codes {
            assert_eq!(code.len(), 9, "{code}");
            assert_eq!(&code[4..5], "-");
            assert_eq!(normalize_backup_code(code).as_deref(), Some(code.as_str()));
            assert_eq!(
                normalize_backup_code(&code.to_uppercase().replace('-', " ")).as_deref(),
                Some(code.as_str())
            );
        }
        assert_eq!(normalize_backup_code("abcd-efg"), None, "too short");
        assert_eq!(
            normalize_backup_code("abcd-ef0l"),
            None,
            "ambiguous letters"
        );
        assert_eq!(normalize_backup_code("123456"), None);
    }

    #[test]
    fn factors_are_read_from_the_request_body() {
        assert!(matches!(
            Factor::from_input(Some("123 456"), None),
            Ok(Factor::Totp(code)) if code == "123456"
        ));
        assert!(matches!(
            Factor::from_input(None, Some("ABCD EFGH")),
            Ok(Factor::Backup(code)) if code == "abcd-efgh"
        ));
        assert!(Factor::from_input(Some("12345"), None).is_err());
        assert!(Factor::from_input(None, Some("nope")).is_err());
        assert!(Factor::from_input(None, None).is_err());
        assert!(Factor::from_input(Some("123456"), Some("abcd-efgh")).is_err());
    }

    #[test]
    fn totp_shape_is_six_digits() {
        assert!(looks_like_totp("123456"));
        assert!(looks_like_totp("123 456"));
        assert!(!looks_like_totp("12345"));
        assert!(!looks_like_totp("abcd-efgh"));
    }
}

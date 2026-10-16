//! Password policy and Argon2id hashing.
//!
//! The policy follows NIST 800-63B: length only, no composition rules. Hashing takes tens of
//! milliseconds on purpose, so it runs on the blocking pool and never on a request thread.

use std::sync::LazyLock;

use argon2::Argon2;
use argon2::password_hash::phc::PasswordHash;
use argon2::password_hash::{PasswordHasher, PasswordVerifier};
use taut_http::AppError;

/// Shortest password accepted.
pub const MIN_CHARS: usize = 8;
/// Longest password accepted; Argon2 would hash anything, but a 1 MB password is abuse.
pub const MAX_CHARS: usize = 128;

/// Hash of a random password, verified against when the email is unknown so a login attempt
/// takes the same time whether or not the account exists.
static DUMMY_HASH: LazyLock<String> = LazyLock::new(|| {
    hash_blocking(&taut_identity::random::opaque_token()).expect("argon2 hashes any input")
});

/// Rejects passwords outside the policy. `email` may not be the password.
pub fn check_policy(password: &str, email: &str) -> Result<(), AppError> {
    let chars = password.chars().count();
    if !(MIN_CHARS..=MAX_CHARS).contains(&chars) {
        return Err(AppError::BadRequest(format!(
            "password must be between {MIN_CHARS} and {MAX_CHARS} characters"
        )));
    }
    if password.trim().eq_ignore_ascii_case(email.trim()) {
        return Err(AppError::BadRequest(
            "password must not be the email address".to_owned(),
        ));
    }
    Ok(())
}

fn hash_blocking(password: &str) -> Result<String, argon2::password_hash::Error> {
    Ok(Argon2::default()
        .hash_password(password.as_bytes())?
        .to_string())
}

fn verify_blocking(password: &str, phc: &str) -> bool {
    PasswordHash::new(phc).is_ok_and(|parsed| {
        Argon2::default()
            .verify_password(password.as_bytes(), &parsed)
            .is_ok()
    })
}

/// Argon2id PHC string of `password`, computed off the async runtime.
pub async fn hash(password: &str) -> Result<String, AppError> {
    let password = password.to_owned();
    let start = std::time::Instant::now();
    let hash = tokio::task::spawn_blocking(move || hash_blocking(&password))
        .await
        .map_err(|err| AppError::Internal(err.into()))?
        .map_err(|err| AppError::Internal(anyhow::anyhow!("argon2: {err}")))?;
    metrics::histogram!("password_hash_duration_seconds").record(start.elapsed().as_secs_f64());
    Ok(hash)
}

/// Whether `password` matches `phc`. With `None`, verifies against a dummy hash so the call
/// takes as long as a real one and always answers `false`.
pub async fn verify(password: &str, phc: Option<&str>) -> Result<bool, AppError> {
    let password = password.to_owned();
    let (phc, real) = match phc {
        Some(phc) => (phc.to_owned(), true),
        None => (DUMMY_HASH.clone(), false),
    };
    let matched = tokio::task::spawn_blocking(move || verify_blocking(&password, &phc))
        .await
        .map_err(|err| AppError::Internal(err.into()))?;
    Ok(matched && real)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn hashes_verify_and_reject_other_passwords() {
        let hash = hash("correct horse battery").await.unwrap();
        assert!(hash.starts_with("$argon2id$"));
        assert!(verify("correct horse battery", Some(&hash)).await.unwrap());
        assert!(!verify("wrong horse", Some(&hash)).await.unwrap());
        assert!(!verify("correct horse battery", None).await.unwrap());
        assert!(!verify("anything", Some("not a phc string")).await.unwrap());
    }

    #[test]
    fn policy_is_length_only_plus_not_the_email() {
        assert!(check_policy("12345678", "a@b.co").is_ok());
        assert!(check_policy(&"x".repeat(128), "a@b.co").is_ok());
        assert!(check_policy("1234567", "a@b.co").is_err());
        assert!(check_policy(&"x".repeat(129), "a@b.co").is_err());
        assert!(check_policy("ana@example.com", "Ana@Example.com").is_err());
        assert!(
            check_policy("ñandúñandú", "a@b.co").is_ok(),
            "counts chars, not bytes"
        );
    }
}

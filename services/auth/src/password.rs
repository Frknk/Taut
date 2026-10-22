//! Password policy and Argon2id hashing.
//!
//! The policy follows NIST 800-63B: length only, no composition rules. Hashing takes tens of
//! milliseconds on purpose, so it runs on the blocking pool and never on a request thread.
//!
//! Each hash also takes 19 MB on purpose (Argon2's default `m`), so only
//! [`AuthSettings::max_concurrent_hashes`] run at a time and the others wait. Without that bound,
//! four logins at once took 76 MB and the 64 MB container was killed: anyone could take login
//! down with a handful of requests, even with wrong passwords. The memory goes back to the system
//! after each hash only with a fixed glibc mmap threshold (`MALLOC_MMAP_THRESHOLD_` in the
//! `Dockerfile`); otherwise glibc keeps 19 MB per hash that ever overlapped another.
//!
//! [`AuthSettings::max_concurrent_hashes`]: taut_config::AuthSettings::max_concurrent_hashes

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{LazyLock, OnceLock};

use tokio::sync::Semaphore;

use argon2::Argon2;
use argon2::password_hash::phc::PasswordHash;
use argon2::password_hash::{PasswordHasher, PasswordVerifier};
use taut_http::AppError;

/// Shortest password accepted.
pub const MIN_CHARS: usize = 8;
/// Longest password accepted; Argon2 would hash anything, but a 1 MB password is abuse.
pub const MAX_CHARS: usize = 128;

/// Hashes allowed at once when [`limit_concurrency`] was not called (tests, tools).
const DEFAULT_CONCURRENT_HASHES: usize = 2;

static HASHES: OnceLock<Semaphore> = OnceLock::new();
/// Hashes running right now, for the `password_hashes_in_flight` gauge.
static IN_FLIGHT: AtomicUsize = AtomicUsize::new(0);

/// Sets how many hashes run at once (at least one). Call it once at startup, before the first
/// hash; later calls are ignored.
pub fn limit_concurrency(max: usize) {
    let _ = HASHES.set(Semaphore::new(max.max(1)));
}

fn permits() -> &'static Semaphore {
    HASHES.get_or_init(|| Semaphore::new(DEFAULT_CONCURRENT_HASHES))
}

/// Runs `work` on the blocking pool once a hash permit is free.
async fn bounded<T: Send + 'static>(
    work: impl FnOnce() -> T + Send + 'static,
) -> Result<T, AppError> {
    let _permit = permits()
        .acquire()
        .await
        .map_err(|err| AppError::Internal(err.into()))?;
    let running = IN_FLIGHT.fetch_add(1, Ordering::SeqCst) + 1;
    #[allow(clippy::cast_precision_loss)] // a handful of hashes
    metrics::gauge!("password_hashes_in_flight").set(running as f64);
    let result = tokio::task::spawn_blocking(work).await;
    let running = IN_FLIGHT.fetch_sub(1, Ordering::SeqCst) - 1;
    #[allow(clippy::cast_precision_loss)] // a handful of hashes
    metrics::gauge!("password_hashes_in_flight").set(running as f64);
    result.map_err(|err| AppError::Internal(err.into()))
}

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
    let hash = bounded(move || hash_blocking(&password))
        .await?
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
    let matched = bounded(move || verify_blocking(&password, &phc)).await?;
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

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn no_more_than_the_limit_hash_at_once() {
        let hash = hash("correct horse battery").await.unwrap();
        let peak = std::sync::Arc::new(AtomicUsize::new(0));
        let watcher = {
            let peak = std::sync::Arc::clone(&peak);
            tokio::spawn(async move {
                loop {
                    peak.fetch_max(IN_FLIGHT.load(Ordering::SeqCst), Ordering::SeqCst);
                    tokio::time::sleep(std::time::Duration::from_millis(1)).await;
                }
            })
        };
        let logins: Vec<_> = (0..8)
            .map(|_| {
                let hash = hash.clone();
                tokio::spawn(async move { verify("correct horse battery", Some(&hash)).await })
            })
            .collect();
        for login in logins {
            assert!(login.await.unwrap().unwrap());
        }
        watcher.abort();
        let peak = peak.load(Ordering::SeqCst);
        assert!(
            (1..=DEFAULT_CONCURRENT_HASHES).contains(&peak),
            "peak of {peak} hashes"
        );
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

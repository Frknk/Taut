//! Signatures of outgoing webhooks.
//!
//! Every generic webhook carries `X-Taut-Signature: t=<unix seconds>,v1=<hex>`, where the hex
//! is `HMAC-SHA256(secret, "<t>." + body)` over the exact bytes sent. The receiver recomputes
//! it with the channel's secret and rejects timestamps older than [`TOLERANCE_SECS`], so a
//! captured request cannot be replayed later. A redelivery is signed again with a fresh
//! timestamp: receivers dedupe on the incident id and event, not on the signature.

use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;

use crate::random;

/// Header that carries the signature.
pub const SIGNATURE_HEADER: &str = "x-taut-signature";
/// Prefix of a signing secret, so it is recognizable when pasted somewhere it should not be.
pub const SECRET_PREFIX: &str = "whsec_";
/// How far a signature's timestamp may be from the receiver's clock.
pub const TOLERANCE_SECS: i64 = 300;

/// Why a signature was rejected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum SignatureError {
    /// The header is not `t=<int>,v1=<hex>`.
    #[error("malformed signature header")]
    Malformed,
    /// The timestamp is outside the tolerance window.
    #[error("signature timestamp outside the tolerance window")]
    Expired,
    /// No `v1` matches the body.
    #[error("signature does not match the body")]
    Mismatch,
}

/// A fresh signing secret: `whsec_` and 256 random bits.
pub fn generate_secret() -> String {
    format!("{SECRET_PREFIX}{}", random::opaque_token())
}

/// The header value for `body` sent at `timestamp` (unix seconds).
pub fn sign(secret: &str, timestamp: i64, body: &[u8]) -> String {
    format!("t={timestamp},v1={}", digest(secret, timestamp, body))
}

/// Checks `header` against `body` at `now` (unix seconds). Accepts any of several `v1`
/// values, so a receiver keeps working while a secret is being rotated on its side.
pub fn verify(secret: &str, header: &str, body: &[u8], now: i64) -> Result<(), SignatureError> {
    let mut timestamp = None;
    let mut candidates = Vec::new();
    for part in header.split(',') {
        match part.trim().split_once('=') {
            Some(("t", value)) => {
                timestamp = Some(
                    value
                        .parse::<i64>()
                        .map_err(|_| SignatureError::Malformed)?,
                );
            }
            Some(("v1", value)) => candidates.push(value),
            _ => {}
        }
    }
    let timestamp = timestamp.ok_or(SignatureError::Malformed)?;
    if candidates.is_empty() {
        return Err(SignatureError::Malformed);
    }
    // abs_diff: a forged `t` near i64::MIN must not overflow.
    if now.abs_diff(timestamp) > TOLERANCE_SECS.unsigned_abs() {
        return Err(SignatureError::Expired);
    }
    let expected = digest(secret, timestamp, body);
    if candidates
        .iter()
        .any(|candidate| random::constant_time_eq(candidate, &expected))
    {
        Ok(())
    } else {
        Err(SignatureError::Mismatch)
    }
}

/// Lowercase hex `HMAC-SHA256(secret, "<timestamp>." + body)`.
fn digest(secret: &str, timestamp: i64, body: &[u8]) -> String {
    let mut mac =
        Hmac::<Sha256>::new_from_slice(secret.as_bytes()).expect("HMAC takes keys of any length");
    mac.update(timestamp.to_string().as_bytes());
    mac.update(b".");
    mac.update(body);
    let mut out = String::with_capacity(64);
    for byte in mac.finalize().into_bytes() {
        use std::fmt::Write as _;
        let _ = write!(out, "{byte:02x}");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const BODY: &[u8] = br#"{"event":"test"}"#;

    #[test]
    fn signatures_match_a_known_vector() {
        // printf '1700000000.{"event":"test"}' | openssl dgst -sha256 -hmac whsec_test
        assert_eq!(
            sign("whsec_test", 1_700_000_000, BODY),
            "t=1700000000,v1=21d2d3606ebbdbf9307ee15e83085df2b83c83dd87cc2e6d2ea6b1cb61afdc3c"
        );
    }

    #[test]
    fn a_signature_verifies_within_the_window() {
        let header = sign("whsec_test", 1_700_000_000, BODY);
        assert_eq!(verify("whsec_test", &header, BODY, 1_700_000_000), Ok(()));
        assert_eq!(verify("whsec_test", &header, BODY, 1_700_000_300), Ok(()));
        assert_eq!(verify("whsec_test", &header, BODY, 1_699_999_700), Ok(()));
    }

    #[test]
    fn a_tampered_body_or_another_secret_is_rejected() {
        let header = sign("whsec_test", 1_700_000_000, BODY);
        assert_eq!(
            verify("whsec_test", &header, br#"{"event":"tset"}"#, 1_700_000_000),
            Err(SignatureError::Mismatch)
        );
        assert_eq!(
            verify("whsec_other", &header, BODY, 1_700_000_000),
            Err(SignatureError::Mismatch)
        );
    }

    #[test]
    fn old_signatures_are_rejected() {
        let header = sign("whsec_test", 1_700_000_000, BODY);
        assert_eq!(
            verify("whsec_test", &header, BODY, 1_700_000_301),
            Err(SignatureError::Expired)
        );
    }

    #[test]
    fn extreme_timestamps_are_expired_not_a_panic() {
        for t in [i64::MIN, i64::MAX] {
            let header = format!("t={t},v1=00");
            assert_eq!(
                verify("whsec_test", &header, BODY, 1_700_000_000),
                Err(SignatureError::Expired),
                "{t}"
            );
        }
    }

    #[test]
    fn any_matching_v1_is_accepted() {
        let good = sign("whsec_test", 1_700_000_000, BODY);
        let v1 = good.split_once(",v1=").unwrap().1;
        let header = format!("t=1700000000,v1=deadbeef,v1={v1}");
        assert_eq!(verify("whsec_test", &header, BODY, 1_700_000_000), Ok(()));
    }

    #[test]
    fn malformed_headers_are_rejected() {
        for header in ["", "garbage", "t=abc,v1=00", "v1=00", "t=1700000000"] {
            assert_eq!(
                verify("whsec_test", header, BODY, 1_700_000_000),
                Err(SignatureError::Malformed),
                "{header:?}"
            );
        }
    }

    #[test]
    fn secrets_are_prefixed_and_unique() {
        let a = generate_secret();
        assert!(a.starts_with(SECRET_PREFIX));
        assert_eq!(a.len(), SECRET_PREFIX.len() + 43);
        assert_ne!(a, generate_secret());
    }
}

//! Opaque secrets and their storage hashes.
//!
//! Refresh tokens, verification links and API keys are 256 random bits encoded as base64url.
//! The database and Redis only keep their SHA-256, so a leaked table does not leak the
//! secrets themselves. Comparisons go through the hash, which is constant-length, so no
//! timing side channel reveals a prefix match.

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use sha2::{Digest, Sha256};

/// 32 bytes from the operating system's CSPRNG.
pub fn token_bytes() -> [u8; 32] {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).expect("the operating system provides randomness");
    bytes
}

/// A fresh opaque secret: 43 base64url characters without padding.
pub fn opaque_token() -> String {
    URL_SAFE_NO_PAD.encode(token_bytes())
}

/// Lowercase hex SHA-256 of `secret`, the form stored and looked up.
pub fn sha256_hex(secret: &str) -> String {
    let digest = Sha256::digest(secret.as_bytes());
    let mut out = String::with_capacity(64);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(out, "{byte:02x}");
    }
    out
}

/// Constant-time equality of two strings, for comparing codes the client typed.
pub fn constant_time_eq(a: &str, b: &str) -> bool {
    use subtle::ConstantTimeEq;
    a.len() == b.len() && a.as_bytes().ct_eq(b.as_bytes()).into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opaque_tokens_are_unique_and_url_safe() {
        let a = opaque_token();
        let b = opaque_token();
        assert_ne!(a, b);
        assert_eq!(a.len(), 43);
        assert!(
            a.bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        );
    }

    #[test]
    fn sha256_hex_matches_a_known_vector() {
        assert_eq!(
            sha256_hex("abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn constant_time_eq_compares_whole_strings() {
        assert!(constant_time_eq("1234-5678", "1234-5678"));
        assert!(!constant_time_eq("1234-5678", "1234-5679"));
        assert!(!constant_time_eq("1234", "1234-5678"));
    }
}

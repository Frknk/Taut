//! Tenant API keys: `taut_live_` followed by 32 random bytes in base64url.
//!
//! The plain key is shown once; the database keeps its SHA-256 and the first characters for
//! display. A bearer token that starts with the prefix is an API key; anything else is a JWT.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::random::{opaque_token, sha256_hex};

/// Prefix of every API key. `live` leaves room for `test` keys later.
pub const PREFIX: &str = "taut_live_";

/// Characters of the key kept in clear for display (`taut_live_Ab12cd34…`).
const DISPLAY_CHARS: usize = 8;

/// What an API key may do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Scope {
    /// `GET` only.
    Read,
    /// Same as a member: reads everything, writes monitors.
    Write,
}

impl Scope {
    /// Lowercase name, as stored.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Write => "write",
        }
    }
}

impl fmt::Display for Scope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A scope name that is neither `read` nor `write`.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("unknown api key scope {0:?}")]
pub struct UnknownScope(pub String);

impl FromStr for Scope {
    type Err = UnknownScope;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "read" => Ok(Self::Read),
            "write" => Ok(Self::Write),
            other => Err(UnknownScope(other.to_owned())),
        }
    }
}

/// A freshly generated key.
#[derive(Debug, Clone)]
pub struct Generated {
    /// The full key, shown to the user once.
    pub plain: String,
    /// SHA-256 of the full key, stored for lookups.
    pub hash: String,
    /// Prefix plus the first characters, stored for display.
    pub display_prefix: String,
}

/// Generates a key.
pub fn generate() -> Generated {
    let plain = format!("{PREFIX}{}", opaque_token());
    let display_prefix = plain[..PREFIX.len() + DISPLAY_CHARS].to_owned();
    Generated {
        hash: sha256_hex(&plain),
        plain,
        display_prefix,
    }
}

/// Whether a bearer credential has the shape of an API key.
pub fn is_api_key(bearer: &str) -> bool {
    bearer.starts_with(PREFIX)
}

/// Hash to look an API key up by, or `None` when the credential is not a key.
pub fn lookup_hash(bearer: &str) -> Option<String> {
    is_api_key(bearer).then(|| sha256_hex(bearer))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_keys_have_prefix_hash_and_display_form() {
        let key = generate();
        assert!(key.plain.starts_with(PREFIX));
        assert_eq!(key.plain.len(), PREFIX.len() + 43);
        assert_eq!(key.hash, sha256_hex(&key.plain));
        assert_eq!(key.display_prefix.len(), PREFIX.len() + DISPLAY_CHARS);
        assert!(key.plain.starts_with(&key.display_prefix));
    }

    #[test]
    fn scopes_round_trip_through_their_names() {
        for scope in [Scope::Read, Scope::Write] {
            assert_eq!(scope.as_str().parse::<Scope>(), Ok(scope));
        }
        assert!("admin".parse::<Scope>().is_err());
    }

    #[test]
    fn only_prefixed_credentials_are_keys() {
        assert!(is_api_key("taut_live_abc"));
        assert!(!is_api_key("eyJhbGciOiJFZERTQSJ9.x.y"));
        assert_eq!(lookup_hash("eyJ.x.y"), None);
        assert_eq!(
            lookup_hash("taut_live_abc"),
            Some(sha256_hex("taut_live_abc"))
        );
    }
}

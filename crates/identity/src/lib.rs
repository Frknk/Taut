//! Identity primitives shared by the auth service (which issues tokens) and the services
//! that verify them.
//!
//! - [`keys::KeyPair`] signs access tokens with Ed25519 and publishes its public half as a
//!   JWK; [`Verifier`] checks tokens against a set of JWKs, static or fetched from the auth
//!   service.
//! - [`Claims`] is the body of an access token.
//! - [`random`] makes the opaque secrets (refresh tokens, verification links) and hashes
//!   them for storage.
//! - [`api_key`] generates and recognizes tenant API keys (`taut_live_…`).
//! - [`bearer`] reads the `Authorization` header and tells a JWT from an API key.
//! - [`webhook`] signs outgoing webhooks (`X-Taut-Signature`) and verifies them.
//!
//! Nothing here touches a database; the only I/O is the optional JWKS fetch.

pub mod api_key;
pub mod bearer;
pub mod claims;
pub mod keys;
pub mod random;
pub mod verifier;
pub mod webhook;

pub use claims::{Claims, TokenKind};
pub use keys::{Jwk, Jwks, KeyPair};
pub use verifier::{Verifier, VerifyError};

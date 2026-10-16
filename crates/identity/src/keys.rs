//! The Ed25519 key pair the auth service signs with, and its public half as a JWK.

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use ed25519_dalek::SigningKey;
use ed25519_dalek::pkcs8::spki::der::pem::LineEnding;
use ed25519_dalek::pkcs8::{DecodePrivateKey, EncodePrivateKey};
use jsonwebtoken::jwk::{
    AlgorithmParameters, CommonParameters, EllipticCurve, KeyAlgorithm, OctetKeyPairParameters,
    OctetKeyPairType, PublicKeyUse,
};
pub use jsonwebtoken::jwk::{Jwk, JwkSet as Jwks};
use jsonwebtoken::{Algorithm, EncodingKey, Header};
use sha2::{Digest, Sha256};

use crate::claims::Claims;
use crate::random::token_bytes;

/// Failure to load a signing key.
#[derive(Debug, thiserror::Error)]
pub enum KeyError {
    /// The PEM is not a PKCS#8 Ed25519 private key.
    #[error("invalid Ed25519 private key: {0}")]
    InvalidPem(String),
    /// The claims could not be signed.
    #[error("signing failed: {0}")]
    Sign(#[from] jsonwebtoken::errors::Error),
}

/// An Ed25519 signing key with the identifier its tokens carry in `kid`.
#[derive(Clone)]
pub struct KeyPair {
    kid: String,
    public: [u8; 32],
    encoding: EncodingKey,
    pem: String,
}

impl std::fmt::Debug for KeyPair {
    // The private key never goes into logs: only its id.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KeyPair")
            .field("kid", &self.kid)
            .finish_non_exhaustive()
    }
}

impl KeyPair {
    /// Loads a PKCS#8 PEM (`openssl genpkey -algorithm ed25519`).
    pub fn from_pem(pem: &str) -> Result<Self, KeyError> {
        let signing =
            SigningKey::from_pkcs8_pem(pem).map_err(|err| KeyError::InvalidPem(err.to_string()))?;
        Ok(Self::from_signing_key(&signing))
    }

    /// A fresh random key. Development only: tokens die with the process.
    pub fn generate() -> Self {
        Self::from_signing_key(&SigningKey::from_bytes(&token_bytes()))
    }

    fn from_signing_key(signing: &SigningKey) -> Self {
        let public = signing.verifying_key().to_bytes();
        let der = signing
            .to_pkcs8_der()
            .expect("an Ed25519 key always encodes as PKCS#8");
        let pem = signing
            .to_pkcs8_pem(LineEnding::LF)
            .expect("an Ed25519 key always encodes as PEM")
            .to_string();
        Self {
            kid: key_id(&public),
            public,
            encoding: EncodingKey::from_ed_der(der.as_bytes()),
            pem,
        }
    }

    /// Identifier of this key, carried by every token it signs.
    pub fn kid(&self) -> &str {
        &self.kid
    }

    /// The private key as PKCS#8 PEM, to persist a generated one.
    pub fn to_pem(&self) -> &str {
        &self.pem
    }

    /// The public half as a JWK.
    pub fn jwk(&self) -> Jwk {
        Jwk {
            common: CommonParameters {
                public_key_use: Some(PublicKeyUse::Signature),
                key_algorithm: Some(KeyAlgorithm::EdDSA),
                key_id: Some(self.kid.clone()),
                ..CommonParameters::default()
            },
            algorithm: AlgorithmParameters::OctetKeyPair(OctetKeyPairParameters {
                key_type: OctetKeyPairType::OctetKeyPair,
                curve: EllipticCurve::Ed25519,
                x: URL_SAFE_NO_PAD.encode(self.public),
            }),
        }
    }

    /// The JWK set the auth service publishes: just this key.
    pub fn jwks(&self) -> Jwks {
        Jwks {
            keys: vec![self.jwk()],
        }
    }

    /// Signs `claims` into a compact JWT with `alg: EdDSA` and this key's `kid`.
    pub fn sign(&self, claims: &Claims) -> Result<String, KeyError> {
        let mut header = Header::new(Algorithm::EdDSA);
        header.kid = Some(self.kid.clone());
        Ok(jsonwebtoken::encode(&header, claims, &self.encoding)?)
    }
}

/// First 16 hex characters of the SHA-256 of the public key: the same key gets the same id on
/// every replica, and a rotated key gets a new one.
fn key_id(public: &[u8; 32]) -> String {
    use std::fmt::Write as _;
    let digest = Sha256::digest(public);
    digest[..8]
        .iter()
        .fold(String::with_capacity(16), |mut out, b| {
            let _ = write!(out, "{b:02x}");
            out
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pem_round_trip_keeps_the_key_id() {
        let generated = KeyPair::generate();
        let loaded = KeyPair::from_pem(generated.to_pem()).unwrap();
        assert_eq!(generated.kid(), loaded.kid());
        assert_eq!(generated.jwk(), loaded.jwk());
        assert_eq!(generated.kid().len(), 16);
    }

    #[test]
    fn jwk_is_an_ed25519_okp_key() {
        let key = KeyPair::generate();
        let json = serde_json::to_value(key.jwks()).unwrap();
        let jwk = &json["keys"][0];
        assert_eq!(jwk["kty"], "OKP");
        assert_eq!(jwk["crv"], "Ed25519");
        assert_eq!(jwk["alg"], "EdDSA");
        assert_eq!(jwk["use"], "sig");
        assert_eq!(jwk["kid"], key.kid());
        assert_eq!(jwk["x"].as_str().unwrap().len(), 43);
    }

    #[test]
    fn garbage_is_not_a_key() {
        assert!(
            KeyPair::from_pem("-----BEGIN PRIVATE KEY-----\nAAAA\n-----END PRIVATE KEY-----\n")
                .is_err()
        );
        assert!(KeyPair::from_pem("not a pem").is_err());
    }
}

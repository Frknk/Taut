//! Outbound HTTP for taut services.
//!
//! Every request taut makes on behalf of a tenant (checks, webhooks) goes through
//! [`guarded_client`]: its resolver drops internal addresses and its redirect policy refuses
//! hops to them, so a tenant cannot point taut at the cluster's own network. TLS uses rustls
//! with `ring`, the provider sqlx also uses.

pub mod resolver;

use std::sync::Arc;

use reqwest::redirect::{Action, Attempt, Policy};
use taut_model::net::is_internal_host;

pub use resolver::{DnsFailure, GuardedResolver, InternalAddress};

/// Redirects followed before a request fails.
pub const MAX_REDIRECTS: usize = 5;

/// Installs `ring` as the process-wide TLS provider. Idempotent.
pub fn install_crypto_provider() {
    // Fails only when a provider is already installed, which is fine.
    let _ = rustls::crypto::ring::default_provider().install_default();
}

/// Client builder with the guarded resolver and redirect policy. `allow_internal` disables
/// both guards (local development, tests).
pub fn guarded_client(allow_internal: bool) -> reqwest::ClientBuilder {
    install_crypto_provider();
    reqwest::Client::builder()
        .dns_resolver(Arc::new(GuardedResolver::new(allow_internal)))
        .redirect(Policy::custom(move |attempt| {
            follow_redirect(attempt, allow_internal)
        }))
}

fn follow_redirect(attempt: Attempt<'_>, allow_internal: bool) -> Action {
    if attempt.previous().len() >= MAX_REDIRECTS {
        let error = format!("more than {MAX_REDIRECTS} redirects");
        return attempt.error(error);
    }
    // IP literals never reach the resolver, so the address rule is applied to the URL too.
    let internal = attempt.url().host_str().is_some_and(is_internal_host);
    if internal && !allow_internal {
        let host = attempt.url().host_str().unwrap_or_default().to_owned();
        return attempt.error(InternalAddress(host));
    }
    attempt.follow()
}

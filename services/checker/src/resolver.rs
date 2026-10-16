//! DNS resolver for the HTTP client that refuses internal addresses.
//!
//! A hostname can resolve to `10.0.0.5` or `169.254.169.254` even when the URL looks public;
//! filtering at resolution time is what keeps monitors from reaching the cluster's network.

use std::net::SocketAddr;

use reqwest::dns::{Addrs, Name, Resolve, Resolving};
use taut_model::net::is_internal_ip;

/// The host only resolves to addresses monitors may not reach.
#[derive(Debug, thiserror::Error)]
#[error("{0} resolves to an internal address")]
pub struct InternalAddress(pub String);

/// The host could not be resolved.
#[derive(Debug, thiserror::Error)]
#[error("dns: could not resolve {0}")]
pub struct DnsFailure(pub String);

/// System resolver that drops internal addresses unless they are allowed.
pub struct GuardedResolver {
    allow_internal: bool,
}

impl GuardedResolver {
    /// `allow_internal` disables the filter (local development, tests).
    pub fn new(allow_internal: bool) -> Self {
        Self { allow_internal }
    }
}

impl Resolve for GuardedResolver {
    fn resolve(&self, name: Name) -> Resolving {
        let allow_internal = self.allow_internal;
        Box::pin(async move {
            let host = name.as_str().to_owned();
            let resolved = tokio::net::lookup_host((host.as_str(), 0))
                .await
                .map_err(|_| DnsFailure(host.clone()))?;
            let allowed: Vec<SocketAddr> = resolved
                .filter(|address| allow_internal || !is_internal_ip(address.ip()))
                .collect();
            if allowed.is_empty() {
                return Err(InternalAddress(host).into());
            }
            let addresses: Addrs = Box::new(allowed.into_iter());
            Ok(addresses)
        })
    }
}

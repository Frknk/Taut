//! HTTP and keyword checks (F-01, F-02) with reqwest.
//!
//! Each check opens a fresh connection (no pooling) so DNS, TCP and TLS problems show up in
//! every result. The target must answer with the expected status (any 2xx by default) and,
//! for keyword monitors, a body that contains the keyword within the first megabyte.

use std::error::Error as StdError;
use std::time::{Duration, Instant};

use taut_model::events::CheckRequested;
use taut_model::net::is_internal_host;
use taut_net::{DnsFailure, InternalAddress};
use url::Url;

use super::{Check, INTERNAL_ADDRESS, Outcome};

/// Largest body read when looking for a keyword.
const MAX_BODY_BYTES: usize = 1024 * 1024;

/// HTTP and keyword check.
pub struct HttpCheck {
    client: reqwest::Client,
    allow_internal: bool,
}

impl HttpCheck {
    /// Builds the HTTP client. `allow_internal` permits loopback and private targets.
    pub fn new(allow_internal: bool) -> Result<Self, reqwest::Error> {
        let client = taut_net::guarded_client(allow_internal)
            .user_agent(concat!("taut-checker/", env!("CARGO_PKG_VERSION")))
            .pool_max_idle_per_host(0)
            .build()?;
        Ok(Self {
            client,
            allow_internal,
        })
    }

    async fn fetch(&self, request: &CheckRequested) -> Result<(u16, Option<bool>), reqwest::Error> {
        let mut response = self.client.get(&request.target).send().await?;
        let status = response.status().as_u16();
        let Some(keyword) = request.keyword.as_deref() else {
            return Ok((status, None));
        };
        let mut body = Vec::new();
        while let Some(chunk) = response.chunk().await? {
            body.extend_from_slice(&chunk);
            if body.len() >= MAX_BODY_BYTES {
                body.truncate(MAX_BODY_BYTES);
                break;
            }
        }
        Ok((
            status,
            Some(String::from_utf8_lossy(&body).contains(keyword)),
        ))
    }
}

impl Check for HttpCheck {
    async fn check(&self, request: &CheckRequested) -> Outcome {
        // IP literals never reach the resolver, so the address rule is applied to the URL too.
        let host = Url::parse(&request.target)
            .ok()
            .and_then(|url| url.host_str().map(ToOwned::to_owned));
        if !self.allow_internal && host.as_deref().is_some_and(is_internal_host) {
            return Outcome::down(Duration::ZERO, None, INTERNAL_ADDRESS);
        }

        let timeout = Duration::from_secs(request.timeout_s.into());
        let start = Instant::now();
        let result = tokio::time::timeout(timeout, self.fetch(request)).await;
        let elapsed = start.elapsed();
        match result {
            Err(_) => Outcome::down(
                timeout,
                None,
                format!("timeout after {} s", request.timeout_s),
            ),
            Ok(Err(err)) => Outcome::down(elapsed, None, describe(&err)),
            Ok(Ok((status, keyword_found))) => {
                let expected = request
                    .expected_status
                    .map_or((200..300).contains(&status), |code| code == status);
                if !expected {
                    let wanted = request
                        .expected_status
                        .map_or_else(|| "2xx".to_owned(), |code| code.to_string());
                    Outcome::down(
                        elapsed,
                        Some(status),
                        format!("status {status} (expected {wanted})"),
                    )
                } else if keyword_found == Some(false) {
                    Outcome::down(elapsed, Some(status), "keyword missing")
                } else {
                    Outcome::up(elapsed, Some(status))
                }
            }
        }
    }
}

/// Most specific readable cause in the error chain of a failed request.
fn describe(err: &reqwest::Error) -> String {
    let mut source: Option<&(dyn StdError + 'static)> = Some(err);
    let mut innermost = err.to_string();
    while let Some(current) = source {
        if current.downcast_ref::<InternalAddress>().is_some() {
            return INTERNAL_ADDRESS.to_owned();
        }
        if let Some(dns) = current.downcast_ref::<DnsFailure>() {
            return dns.to_string();
        }
        if let Some(io) = current.downcast_ref::<std::io::Error>() {
            match io.kind() {
                std::io::ErrorKind::ConnectionRefused => return "connection refused".to_owned(),
                std::io::ErrorKind::ConnectionReset => return "connection reset".to_owned(),
                _ => {}
            }
        }
        innermost = current.to_string();
        source = current.source();
    }
    if err.is_redirect() {
        return innermost;
    }
    let lowered = innermost.to_ascii_lowercase();
    if lowered.contains("certificate") || lowered.contains("tls") || lowered.contains("handshake") {
        return format!("tls: {innermost}");
    }
    if err.is_connect() {
        return format!("connection failed: {innermost}");
    }
    innermost
}

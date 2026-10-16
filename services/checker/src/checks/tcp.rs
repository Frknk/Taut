//! TCP port check (F-04): healthy when a connection to `host:port` opens in time.

use std::net::SocketAddr;
use std::time::{Duration, Instant};

use taut_model::events::CheckRequested;
use taut_model::net::is_internal_ip;
use tokio::net::TcpStream;

use super::{Check, INTERNAL_ADDRESS, Outcome};

/// TCP connect check.
pub struct TcpCheck {
    allow_internal: bool,
}

impl TcpCheck {
    /// `allow_internal` permits loopback and private targets.
    pub fn new(allow_internal: bool) -> Self {
        Self { allow_internal }
    }

    async fn connect(&self, target: &str) -> Result<(), String> {
        let host = target.rsplit_once(':').map_or(target, |(host, _)| host);
        let addresses: Vec<SocketAddr> = tokio::net::lookup_host(target)
            .await
            .map_err(|_| format!("dns: could not resolve {host}"))?
            .filter(|address| self.allow_internal || !is_internal_ip(address.ip()))
            .collect();
        if addresses.is_empty() {
            return Err(INTERNAL_ADDRESS.to_owned());
        }
        let mut last_error = None;
        for address in addresses {
            match TcpStream::connect(address).await {
                Ok(_) => return Ok(()),
                Err(err) => last_error = Some(err),
            }
        }
        Err(match last_error {
            Some(err) if err.kind() == std::io::ErrorKind::ConnectionRefused => {
                "connection refused".to_owned()
            }
            Some(err) => format!("connection failed: {err}"),
            None => "connection failed".to_owned(),
        })
    }
}

impl Check for TcpCheck {
    async fn check(&self, request: &CheckRequested) -> Outcome {
        let timeout = Duration::from_secs(request.timeout_s.into());
        let start = Instant::now();
        match tokio::time::timeout(timeout, self.connect(&request.target)).await {
            Err(_) => Outcome::down(
                timeout,
                None,
                format!("timeout after {} s", request.timeout_s),
            ),
            Ok(Err(error)) => Outcome::down(start.elapsed(), None, error),
            Ok(Ok(())) => Outcome::up(start.elapsed(), None),
        }
    }
}

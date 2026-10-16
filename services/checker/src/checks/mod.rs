//! One module per monitor type, each implementing [`Check`].
//!
//! Adding a type (the DNS monitor of week 12) means a new module here, a match arm in
//! [`Checker::run`], the value in `taut_model::MonitorKind` and its validation in the API.

pub mod http;
pub mod tcp;

use std::future::Future;
use std::time::Duration;

use taut_model::MonitorKind;
use taut_model::events::CheckRequested;

/// Result of running one check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outcome {
    /// Whether the target was healthy.
    pub ok: bool,
    /// HTTP status code, when there was a response.
    pub status: Option<u16>,
    /// Milliseconds until the answer or the failure.
    pub latency_ms: u32,
    /// Readable cause of a failure.
    pub error: Option<String>,
}

impl Outcome {
    /// Healthy target.
    pub fn up(latency: Duration, status: Option<u16>) -> Self {
        Self {
            ok: true,
            status,
            latency_ms: millis(latency),
            error: None,
        }
    }

    /// Failed check with its cause.
    pub fn down(latency: Duration, status: Option<u16>, error: impl Into<String>) -> Self {
        Self {
            ok: false,
            status,
            latency_ms: millis(latency),
            error: Some(error.into()),
        }
    }
}

fn millis(duration: Duration) -> u32 {
    u32::try_from(duration.as_millis()).unwrap_or(u32::MAX)
}

/// A kind of check.
pub trait Check {
    /// Runs one check. Never fails: every problem becomes a failed [`Outcome`] whose `error`
    /// says why, because the cause is what the user sees in the incident.
    fn check(&self, request: &CheckRequested) -> impl Future<Output = Outcome> + Send;
}

/// Error message shared by every check that refuses an internal address.
pub const INTERNAL_ADDRESS: &str = "target resolves to an internal address";

/// Every check, dispatched by monitor type.
pub struct Checker {
    http: http::HttpCheck,
    tcp: tcp::TcpCheck,
}

impl Checker {
    /// Builds the checks. `allow_internal` permits loopback and private targets.
    pub fn new(allow_internal: bool) -> Result<Self, reqwest::Error> {
        Ok(Self {
            http: http::HttpCheck::new(allow_internal)?,
            tcp: tcp::TcpCheck::new(allow_internal),
        })
    }

    /// Runs the check that matches `request.kind`.
    pub async fn run(&self, request: &CheckRequested) -> Outcome {
        match request.kind {
            MonitorKind::Http | MonitorKind::Keyword => self.http.check(request).await,
            MonitorKind::Tcp => self.tcp.check(request).await,
            MonitorKind::Heartbeat => Outcome::down(
                Duration::ZERO,
                None,
                "heartbeats are evaluated by the scheduler, not checked",
            ),
        }
    }
}

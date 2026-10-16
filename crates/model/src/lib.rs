//! Domain types shared by taut services.
//!
//! Everything here is plain data with no I/O: what a monitor is, the states it can be in, the
//! events services exchange and the rules that more than one service must apply the same way. Enums are stored as `text`
//! in PostgreSQL and serialized in lowercase, so the database, the API and the bus agree on
//! the spelling.

pub mod events;
pub mod net;

use std::fmt;

use serde::{Deserialize, Serialize};

/// What a monitor checks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, sqlx::Type)]
#[serde(rename_all = "lowercase")]
#[sqlx(type_name = "text", rename_all = "lowercase")]
pub enum MonitorKind {
    /// HTTP(S) request; healthy when the status code matches.
    Http,
    /// HTTP(S) request; healthy when the status matches and the body contains a keyword.
    Keyword,
    /// TCP connection to `host:port`.
    Tcp,
    /// Inbound: the monitored job pings taut, and silence is the failure.
    Heartbeat,
}

impl MonitorKind {
    /// Every kind, in the order the UI lists them.
    pub const ALL: [Self; 4] = [Self::Http, Self::Keyword, Self::Tcp, Self::Heartbeat];

    /// Lowercase name, as stored and serialized.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Http => "http",
            Self::Keyword => "keyword",
            Self::Tcp => "tcp",
            Self::Heartbeat => "heartbeat",
        }
    }

    /// Whether the check is an HTTP request (`http` or `keyword`).
    pub const fn is_http(self) -> bool {
        matches!(self, Self::Http | Self::Keyword)
    }
}

impl fmt::Display for MonitorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Health of a monitor as shown to users.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, sqlx::Type)]
#[serde(rename_all = "lowercase")]
#[sqlx(type_name = "text", rename_all = "lowercase")]
pub enum MonitorStatus {
    /// No conclusive result yet: just created, or a heartbeat that never pinged.
    Pending,
    /// Last conclusive result was healthy.
    Up,
    /// An incident is open.
    Down,
    /// Paused by the user; not checked.
    Paused,
}

impl MonitorStatus {
    /// Lowercase name, as stored and serialized.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Up => "up",
            Self::Down => "down",
            Self::Paused => "paused",
        }
    }
}

impl fmt::Display for MonitorStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Percentage of healthy checks, `None` when there were none. Used by every uptime figure
/// (panel, status page) so they always agree.
pub fn uptime_percent(checks: i64, failures: i64) -> Option<f64> {
    if checks <= 0 {
        return None;
    }
    #[allow(clippy::cast_precision_loss)] // counts far below 2^52
    let ratio = (checks - failures.clamp(0, checks)) as f64 / checks as f64;
    Some(ratio * 100.0)
}

/// Where an alert goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, sqlx::Type)]
#[serde(rename_all = "lowercase")]
#[sqlx(type_name = "text", rename_all = "lowercase")]
pub enum ChannelKind {
    /// Email through SMTP; config `{"to": "ops@example.com"}`.
    Email,
    /// Generic webhook with the full JSON event; config `{"url": "https://..."}`.
    Webhook,
    /// Discord incoming webhook (`{"content": ...}`); config `{"url": "https://..."}`.
    Discord,
    /// Slack incoming webhook (`{"text": ...}`); config `{"url": "https://..."}`.
    Slack,
}

impl ChannelKind {
    /// Lowercase name, as stored and serialized.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Email => "email",
            Self::Webhook => "webhook",
            Self::Discord => "discord",
            Self::Slack => "slack",
        }
    }
}

impl fmt::Display for ChannelKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_serialize_as_their_name() {
        for kind in MonitorKind::ALL {
            let json = serde_json::to_string(&kind).unwrap();
            assert_eq!(json, format!("\"{kind}\""));
            assert_eq!(serde_json::from_str::<MonitorKind>(&json).unwrap(), kind);
        }
    }

    #[test]
    fn statuses_serialize_as_their_name() {
        let json = serde_json::to_string(&MonitorStatus::Paused).unwrap();
        assert_eq!(json, "\"paused\"");
    }

    #[test]
    fn uptime_needs_checks() {
        assert_eq!(uptime_percent(0, 0), None);
        assert_eq!(uptime_percent(4, 1), Some(75.0));
        assert_eq!(uptime_percent(4, 9), Some(0.0));
    }

    #[test]
    fn only_http_kinds_are_http() {
        assert!(MonitorKind::Http.is_http());
        assert!(MonitorKind::Keyword.is_http());
        assert!(!MonitorKind::Tcp.is_http());
        assert!(!MonitorKind::Heartbeat.is_http());
    }
}

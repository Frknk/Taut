//! Events that travel on the bus (NATS JetStream), with their subjects.
//!
//! Every event carries the `trace_id` born when the work started (the scheduler for a check,
//! the API for a heartbeat ping), so one search in the logs follows a check from scheduler to
//! checker to incident to notification. The bus also copies it into a message header.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::MonitorKind;

/// Subjects of every event, grouped by stream.
pub mod subjects {
    /// The scheduler asks for one check ([`super::CheckRequested`]).
    pub const CHECKS_REQUESTED: &str = "checks.requested";
    /// A check finished or a heartbeat reported in ([`super::CheckCompleted`]).
    pub const CHECKS_COMPLETED: &str = "checks.completed";
    /// A monitor was created ([`super::MonitorChanged`]).
    pub const MONITORS_CREATED: &str = "monitors.created";
    /// A monitor's configuration or active flag changed ([`super::MonitorChanged`]).
    pub const MONITORS_UPDATED: &str = "monitors.updated";
    /// A monitor was deleted ([`super::MonitorChanged`]).
    pub const MONITORS_DELETED: &str = "monitors.deleted";
    /// Wildcard for every monitor event.
    pub const MONITORS_ALL: &str = "monitors.>";
    /// An incident opened ([`super::IncidentChanged`]).
    pub const INCIDENTS_OPENED: &str = "incidents.opened";
    /// An incident was resolved ([`super::IncidentChanged`]).
    pub const INCIDENTS_RESOLVED: &str = "incidents.resolved";
    /// Wildcard for every incident event.
    pub const INCIDENTS_ALL: &str = "incidents.>";
    /// A user asked to test an alert channel ([`super::TestNotification`]).
    pub const NOTIFICATIONS_TEST: &str = "notifications.test";
}

/// One check to run. Carries everything the checker needs, so checkers never read the
/// database and can run in any region.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckRequested {
    /// Trace id of this check, a ULID.
    pub trace_id: String,
    /// Monitor being checked.
    pub monitor_id: Uuid,
    /// Owner of the monitor.
    pub tenant_id: Uuid,
    /// What to check; never `heartbeat` (the scheduler evaluates heartbeats itself).
    pub kind: MonitorKind,
    /// URL or `host:port`.
    pub target: String,
    /// Seconds before the check counts as failed.
    pub timeout_s: u32,
    /// HTTP status that counts as healthy; `None` means any 2xx.
    pub expected_status: Option<u16>,
    /// Text the body must contain (`keyword`).
    pub keyword: Option<String>,
    /// When the check was due.
    pub scheduled_at: DateTime<Utc>,
    /// After this instant the next check is due and this one is useless; checkers drop it.
    pub expires_at: DateTime<Utc>,
}

/// Result of one check, or of a heartbeat ping or its absence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckCompleted {
    /// Trace id of the check.
    pub trace_id: String,
    /// Monitor that was checked.
    pub monitor_id: Uuid,
    /// Owner of the monitor.
    pub tenant_id: Uuid,
    /// When the check started (or the ping arrived). With the region it identifies the
    /// result, which makes recording it idempotent.
    pub at: DateTime<Utc>,
    /// Region of the checker, or of the API for heartbeats.
    pub region: String,
    /// Whether the target was healthy.
    pub ok: bool,
    /// HTTP status code, when there was a response.
    pub status: Option<u16>,
    /// Milliseconds until the answer (or the failure).
    pub latency_ms: Option<u32>,
    /// Readable cause of a failure: `timeout after 10 s`, `connection refused`,
    /// `status 500 (expected 2xx)`, `keyword missing`, ...
    pub error: Option<String>,
}

/// A monitor changed; the subject says how. Consumers read the current state from the
/// database, so the payload is only the identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MonitorChanged {
    /// Monitor that changed.
    pub monitor_id: Uuid,
    /// Owner of the monitor.
    pub tenant_id: Uuid,
}

/// An incident opened or was resolved; the subject says which. Carries what an alert shows,
/// so the notifier does not need to read the monitor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IncidentChanged {
    /// Trace id of the check that caused the transition.
    pub trace_id: String,
    /// Incident id.
    pub incident_id: Uuid,
    /// Owner of the monitor.
    pub tenant_id: Uuid,
    /// Monitor that failed.
    pub monitor_id: Uuid,
    /// Display name of the monitor.
    pub monitor_name: String,
    /// First failure of the streak that opened the incident.
    pub started_at: DateTime<Utc>,
    /// When it was resolved; `None` on `incidents.opened`.
    pub resolved_at: Option<DateTime<Utc>>,
    /// Cause of the first failure.
    pub cause: String,
}

/// Request to send a test message through one alert channel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TestNotification {
    /// Trace id of the API request.
    pub trace_id: String,
    /// Owner of the channel.
    pub tenant_id: Uuid,
    /// Channel to test.
    pub channel_id: Uuid,
}

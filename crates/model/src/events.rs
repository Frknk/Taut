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
    /// Someone registered or asked to verify their email ([`super::AccountEmail`]).
    pub const USERS_VERIFICATION_REQUESTED: &str = "users.verification_requested";
    /// Someone asked to reset their password ([`super::AccountEmail`]).
    pub const USERS_PASSWORD_RESET_REQUESTED: &str = "users.password_reset_requested";
    /// Someone registered with an email that already has an account ([`super::AccountEmail`]).
    pub const USERS_ALREADY_REGISTERED: &str = "users.already_registered";
    /// Wildcard for every user event.
    pub const USERS_ALL: &str = "users.>";
    /// An owner invited someone to a tenant ([`super::AccountEmail`]).
    pub const TENANTS_MEMBER_INVITED: &str = "tenants.member_invited";
    /// Wildcard for every tenant event.
    pub const TENANTS_ALL: &str = "tenants.>";
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

/// An email about an account that the notifier renders and sends. The producer builds the
/// link, so the panel route lives next to the endpoint that consumes the token; the notifier
/// only writes the text. The payload carries a secret: never log it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AccountEmail {
    /// Confirm the address after registering ([`subjects::USERS_VERIFICATION_REQUESTED`]).
    VerificationRequested {
        /// Trace id of the request.
        trace_id: String,
        /// Recipient.
        to: String,
        /// Display name of the recipient.
        name: String,
        /// Panel URL that completes the verification.
        link: String,
        /// When the link stops working.
        expires_at: DateTime<Utc>,
    },
    /// Choose a new password ([`subjects::USERS_PASSWORD_RESET_REQUESTED`]).
    PasswordResetRequested {
        /// Trace id of the request.
        trace_id: String,
        /// Recipient.
        to: String,
        /// Display name of the recipient.
        name: String,
        /// Panel URL that opens the reset form.
        link: String,
        /// When the link stops working.
        expires_at: DateTime<Utc>,
    },
    /// Someone tried to register an email that already has an account
    /// ([`subjects::USERS_ALREADY_REGISTERED`]). Sent instead of revealing it in the response.
    AlreadyRegistered {
        /// Trace id of the request.
        trace_id: String,
        /// Recipient.
        to: String,
        /// Display name of the existing account.
        name: String,
        /// Panel URL of the login page.
        login_link: String,
        /// Panel URL of the "forgot password" page.
        reset_link: String,
    },
    /// An invitation to join a tenant ([`subjects::TENANTS_MEMBER_INVITED`]).
    MemberInvited {
        /// Trace id of the request.
        trace_id: String,
        /// Recipient.
        to: String,
        /// Tenant that invites.
        tenant_id: Uuid,
        /// Display name of the tenant.
        tenant_name: String,
        /// Display name of the owner who invited.
        invited_by: String,
        /// Role offered, `owner` or `member`.
        role: String,
        /// Panel URL that accepts the invitation.
        link: String,
        /// When the invitation stops working.
        expires_at: DateTime<Utc>,
    },
}

impl AccountEmail {
    /// Subject this email travels on.
    pub const fn subject(&self) -> &'static str {
        match self {
            Self::VerificationRequested { .. } => subjects::USERS_VERIFICATION_REQUESTED,
            Self::PasswordResetRequested { .. } => subjects::USERS_PASSWORD_RESET_REQUESTED,
            Self::AlreadyRegistered { .. } => subjects::USERS_ALREADY_REGISTERED,
            Self::MemberInvited { .. } => subjects::TENANTS_MEMBER_INVITED,
        }
    }

    /// Recipient address.
    pub fn to(&self) -> &str {
        match self {
            Self::VerificationRequested { to, .. }
            | Self::PasswordResetRequested { to, .. }
            | Self::AlreadyRegistered { to, .. }
            | Self::MemberInvited { to, .. } => to,
        }
    }

    /// Trace id of the request that produced it.
    pub fn trace_id(&self) -> &str {
        match self {
            Self::VerificationRequested { trace_id, .. }
            | Self::PasswordResetRequested { trace_id, .. }
            | Self::AlreadyRegistered { trace_id, .. }
            | Self::MemberInvited { trace_id, .. } => trace_id,
        }
    }
}

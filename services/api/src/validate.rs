//! Validation of monitor input against the rules of prototype 1 (docs/prototipo-1.md).
//!
//! Pure functions, so every rule is unit-tested without a database. The schema repeats the
//! essential constraints as `CHECK`s; this module is what turns bad input into a readable
//! message and fills in the defaults.

use std::net::{Ipv4Addr, Ipv6Addr};

use serde::Deserialize;
use serde_json::{Value, json};
use taut_model::net::is_internal_host;
use taut_model::{ChannelKind, MonitorKind};
use url::Url;

/// Check intervals the API accepts, in seconds (F-14).
pub const INTERVALS_S: [i32; 4] = [30, 60, 180, 300];
/// Interval used when the client sends none.
pub const DEFAULT_INTERVAL_S: i32 = 60;
/// Timeout used when the client sends none (F-15).
pub const DEFAULT_TIMEOUT_S: i32 = 10;
/// Longest timeout accepted; it must also be shorter than the interval.
pub const MAX_TIMEOUT_S: i32 = 60;
/// Default confirmation period (F-16): with a 60 s interval, two failures in a row.
pub const DEFAULT_CONFIRMATION_PERIOD_S: i32 = 60;
/// Default recovery period (F-17): with a 60 s interval, two successes in a row.
pub const DEFAULT_RECOVERY_PERIOD_S: i32 = 120;
/// Default extra silence a heartbeat tolerates after its interval (F-03).
pub const DEFAULT_GRACE_PERIOD_S: i32 = 60;
/// Upper bound for confirmation, recovery and grace periods.
pub const MAX_PERIOD_S: i32 = 3600;

const MAX_NAME_CHARS: usize = 100;
const MAX_KEYWORD_CHARS: usize = 200;
const MAX_TARGET_CHARS: usize = 2048;

/// Body of `POST /v1/monitors` and `PUT /v1/monitors/{id}`.
///
/// Optional numeric fields take their default when absent. Unknown fields are rejected so a
/// typo such as `intervall_s` is an error instead of a silent default.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MonitorInput {
    /// Display name, shown in the panel, alerts and the status page.
    pub name: String,
    /// What the monitor checks. Serialized as `type`.
    #[serde(rename = "type")]
    pub kind: MonitorKind,
    /// URL (`http`, `keyword`) or `host:port` (`tcp`); absent for heartbeats.
    pub target: Option<String>,
    /// Seconds between checks.
    pub interval_s: Option<i32>,
    /// Seconds before a check counts as failed.
    pub timeout_s: Option<i32>,
    /// HTTP status that counts as healthy; absent means any 2xx.
    pub expected_status: Option<i32>,
    /// Text the response body must contain (`keyword` monitors).
    pub keyword: Option<String>,
    /// Seconds a failure must persist before an incident opens.
    pub confirmation_period_s: Option<i32>,
    /// Seconds of consecutive successes before an incident closes.
    pub recovery_period_s: Option<i32>,
    /// Heartbeats: extra seconds of silence tolerated after the interval.
    pub grace_period_s: Option<i32>,
}

/// Monitor settings after validation, with defaults applied and the target normalized.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MonitorSpec {
    /// Trimmed display name.
    pub name: String,
    /// What the monitor checks.
    pub kind: MonitorKind,
    /// Normalized target; `None` for heartbeats.
    pub target: Option<String>,
    /// Seconds between checks.
    pub interval_s: i32,
    /// Seconds before a check counts as failed.
    pub timeout_s: i32,
    /// HTTP status that counts as healthy; `None` means any 2xx.
    pub expected_status: Option<i16>,
    /// Text the body must contain; only for `keyword`.
    pub keyword: Option<String>,
    /// Seconds a failure must persist before an incident opens.
    pub confirmation_period_s: i32,
    /// Seconds of consecutive successes before an incident closes.
    pub recovery_period_s: i32,
    /// Heartbeats: extra seconds of silence tolerated after the interval.
    pub grace_period_s: i32,
}

/// Validates `input`. `allow_internal` permits targets on loopback or private networks.
///
/// The error is a message for the client, naming the offending field.
pub fn monitor(input: MonitorInput, allow_internal: bool) -> Result<MonitorSpec, String> {
    let name = input.name.trim();
    if name.is_empty() {
        return Err("name is required".to_owned());
    }
    if name.chars().count() > MAX_NAME_CHARS {
        return Err(format!("name must be at most {MAX_NAME_CHARS} characters"));
    }

    let interval_s = input.interval_s.unwrap_or(DEFAULT_INTERVAL_S);
    if !INTERVALS_S.contains(&interval_s) {
        return Err("interval_s must be one of 30, 60, 180 or 300".to_owned());
    }
    let timeout_s = input.timeout_s.unwrap_or(DEFAULT_TIMEOUT_S);
    if !(1..=MAX_TIMEOUT_S).contains(&timeout_s) {
        return Err(format!("timeout_s must be between 1 and {MAX_TIMEOUT_S}"));
    }
    if timeout_s >= interval_s {
        return Err("timeout_s must be shorter than interval_s".to_owned());
    }
    let confirmation_period_s = period(
        "confirmation_period_s",
        input.confirmation_period_s,
        DEFAULT_CONFIRMATION_PERIOD_S,
    )?;
    let recovery_period_s = period(
        "recovery_period_s",
        input.recovery_period_s,
        DEFAULT_RECOVERY_PERIOD_S,
    )?;
    let grace_period_s = period(
        "grace_period_s",
        input.grace_period_s,
        DEFAULT_GRACE_PERIOD_S,
    )?;

    let kind = input.kind;
    let target = match (kind, input.target.as_deref()) {
        (MonitorKind::Heartbeat, None) => None,
        (MonitorKind::Heartbeat, Some(_)) => {
            return Err("heartbeat monitors have no target".to_owned());
        }
        (_, None) => return Err("target is required".to_owned()),
        (MonitorKind::Tcp, Some(raw)) => Some(tcp_target(raw, allow_internal)?),
        (MonitorKind::Http | MonitorKind::Keyword, Some(raw)) => {
            Some(http_target(raw, allow_internal)?)
        }
    };

    let expected_status = match input.expected_status {
        None => None,
        Some(_) if !kind.is_http() => {
            return Err("expected_status only applies to http and keyword monitors".to_owned());
        }
        Some(status) => Some(
            i16::try_from(status)
                .ok()
                .filter(|status| (100..=599).contains(status))
                .ok_or("expected_status must be an HTTP status code between 100 and 599")?,
        ),
    };

    let keyword = match (kind, input.keyword) {
        (MonitorKind::Keyword, None) => return Err("keyword is required".to_owned()),
        (MonitorKind::Keyword, Some(keyword)) => {
            if keyword.trim().is_empty() {
                return Err("keyword is required".to_owned());
            }
            if keyword.chars().count() > MAX_KEYWORD_CHARS {
                return Err(format!(
                    "keyword must be at most {MAX_KEYWORD_CHARS} characters"
                ));
            }
            Some(keyword)
        }
        (_, None) => None,
        (_, Some(_)) => return Err("keyword only applies to keyword monitors".to_owned()),
    };

    Ok(MonitorSpec {
        name: name.to_owned(),
        kind,
        target,
        interval_s,
        timeout_s,
        expected_status,
        keyword,
        confirmation_period_s,
        recovery_period_s,
        grace_period_s,
    })
}

fn period(field: &str, value: Option<i32>, default: i32) -> Result<i32, String> {
    let value = value.unwrap_or(default);
    if (0..=MAX_PERIOD_S).contains(&value) {
        Ok(value)
    } else {
        Err(format!("{field} must be between 0 and {MAX_PERIOD_S}"))
    }
}

fn internal_target() -> String {
    "target must be a public address; localhost and private networks are not allowed".to_owned()
}

/// Absolute `http`/`https` URL, normalized by the `url` crate.
fn http_target(raw: &str, allow_internal: bool) -> Result<String, String> {
    let raw = raw.trim();
    if raw.chars().count() > MAX_TARGET_CHARS {
        return Err(format!(
            "target must be at most {MAX_TARGET_CHARS} characters"
        ));
    }
    let invalid = || "target must be an absolute http or https URL".to_owned();
    let url = Url::parse(raw).map_err(|_| invalid())?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(invalid());
    }
    let host = url.host_str().ok_or_else(invalid)?;
    if !allow_internal && is_internal_host(host) {
        return Err(internal_target());
    }
    Ok(url.into())
}

/// `host:port` with a hostname, an IPv4 address or a bracketed IPv6 address.
fn tcp_target(raw: &str, allow_internal: bool) -> Result<String, String> {
    let invalid = || "target must be host:port, for example db.example.com:5432".to_owned();
    let raw = raw.trim().to_ascii_lowercase();
    let (host, port) = raw.rsplit_once(':').ok_or_else(invalid)?;
    let port = port
        .parse::<u16>()
        .ok()
        .filter(|port| *port != 0)
        .ok_or_else(invalid)?;
    let valid = match host.strip_prefix('[') {
        Some(rest) => rest
            .strip_suffix(']')
            .is_some_and(|ip| ip.parse::<Ipv6Addr>().is_ok()),
        None => host.parse::<Ipv4Addr>().is_ok() || is_hostname(host),
    };
    if !valid {
        return Err(invalid());
    }
    if !allow_internal && is_internal_host(host) {
        return Err(internal_target());
    }
    Ok(format!("{host}:{port}"))
}

fn is_hostname(host: &str) -> bool {
    !host.is_empty()
        && host.len() <= 253
        && host.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
        })
}

/// Body of `POST /v1/alert-channels` and `PUT /v1/alert-channels/{id}`.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChannelInput {
    /// Display name.
    pub name: String,
    /// Where alerts go. Serialized as `type`.
    #[serde(rename = "type")]
    pub kind: ChannelKind,
    /// `{"to": "ops@example.com"}` for email, `{"url": "https://..."}` for the webhook kinds.
    pub config: Value,
    /// `false` mutes the channel without deleting it; `true` when absent.
    pub active: Option<bool>,
}

/// Alert channel after validation, with its config normalized.
#[derive(Debug, Clone, PartialEq)]
pub struct ChannelSpec {
    /// Trimmed display name.
    pub name: String,
    /// Where alerts go.
    pub kind: ChannelKind,
    /// Normalized config with only the known key.
    pub config: Value,
    /// Whether alerts are sent.
    pub active: bool,
}

const MAX_EMAIL_CHARS: usize = 254;

/// Validates an alert channel. `allow_internal` permits webhooks on private networks.
pub fn channel(input: &ChannelInput, allow_internal: bool) -> Result<ChannelSpec, String> {
    let name = input.name.trim();
    if name.is_empty() || name.chars().count() > MAX_NAME_CHARS {
        return Err(format!(
            "name must be between 1 and {MAX_NAME_CHARS} characters"
        ));
    }
    let field = |key: &str| {
        input
            .config
            .get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
    };
    let config = match input.kind {
        ChannelKind::Email => {
            let to = field("to").ok_or("config.to is required for email channels")?;
            if !is_email(to) {
                return Err("config.to must be an email address".to_owned());
            }
            json!({ "to": to })
        }
        ChannelKind::Webhook | ChannelKind::Discord | ChannelKind::Slack => {
            let url = field("url").ok_or("config.url is required for webhook channels")?;
            let url = http_target(url, allow_internal)
                .map_err(|err| err.replacen("target", "config.url", 1))?;
            json!({ "url": url })
        }
    };
    Ok(ChannelSpec {
        name: name.to_owned(),
        kind: input.kind,
        config,
        active: input.active.unwrap_or(true),
    })
}

/// One `@`, something on each side, a dot in the domain and no whitespace. The SMTP server
/// is the real judge; this only catches typos.
pub fn is_email(value: &str) -> bool {
    value.len() <= MAX_EMAIL_CHARS
        && !value.chars().any(char::is_whitespace)
        && value.split_once('@').is_some_and(|(local, domain)| {
            !local.is_empty()
                && !domain.contains('@')
                && domain.contains('.')
                && !domain.starts_with('.')
                && !domain.ends_with('.')
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(kind: MonitorKind, target: Option<&str>) -> MonitorInput {
        MonitorInput {
            name: "Sitio".to_owned(),
            kind,
            target: target.map(ToOwned::to_owned),
            interval_s: None,
            timeout_s: None,
            expected_status: None,
            keyword: None,
            confirmation_period_s: None,
            recovery_period_s: None,
            grace_period_s: None,
        }
    }

    fn http(target: &str) -> MonitorInput {
        input(MonitorKind::Http, Some(target))
    }

    fn error(input: MonitorInput) -> String {
        monitor(input, false).expect_err("input should be rejected")
    }

    #[test]
    fn http_monitor_gets_the_documented_defaults() {
        let spec = monitor(http(" https://example.com "), false).unwrap();
        assert_eq!(spec.name, "Sitio");
        assert_eq!(spec.target.as_deref(), Some("https://example.com/"));
        assert_eq!(spec.interval_s, 60);
        assert_eq!(spec.timeout_s, 10);
        assert_eq!(spec.confirmation_period_s, 60);
        assert_eq!(spec.recovery_period_s, 120);
        assert_eq!(spec.grace_period_s, 60);
        assert_eq!(spec.expected_status, None);
    }

    #[test]
    fn name_is_required_and_bounded() {
        let mut blank = http("https://example.com");
        blank.name = "   ".to_owned();
        assert_eq!(error(blank), "name is required");

        let mut long = http("https://example.com");
        long.name = "x".repeat(101);
        assert!(error(long).starts_with("name must be at most"));
    }

    #[test]
    fn only_the_four_intervals_are_accepted() {
        for interval in INTERVALS_S {
            let mut ok = http("https://example.com");
            ok.interval_s = Some(interval);
            assert!(monitor(ok, false).is_ok(), "{interval} s should be valid");
        }
        let mut odd = http("https://example.com");
        odd.interval_s = Some(45);
        assert!(error(odd).starts_with("interval_s must be one of"));
    }

    #[test]
    fn timeout_must_be_shorter_than_the_interval() {
        let mut equal = http("https://example.com");
        equal.interval_s = Some(30);
        equal.timeout_s = Some(30);
        assert_eq!(error(equal), "timeout_s must be shorter than interval_s");

        let mut zero = http("https://example.com");
        zero.timeout_s = Some(0);
        assert!(error(zero).starts_with("timeout_s must be between"));

        let mut huge = http("https://example.com");
        huge.interval_s = Some(300);
        huge.timeout_s = Some(120);
        assert!(error(huge).starts_with("timeout_s must be between"));
    }

    #[test]
    fn periods_are_bounded() {
        let mut negative = http("https://example.com");
        negative.confirmation_period_s = Some(-1);
        assert!(error(negative).starts_with("confirmation_period_s"));

        let mut long = http("https://example.com");
        long.recovery_period_s = Some(3601);
        assert!(error(long).starts_with("recovery_period_s"));

        let mut immediate = http("https://example.com");
        immediate.confirmation_period_s = Some(0);
        assert_eq!(monitor(immediate, false).unwrap().confirmation_period_s, 0);
    }

    #[test]
    fn http_targets_must_be_absolute_web_urls() {
        assert!(error(http("example.com")).contains("absolute http or https URL"));
        assert!(error(http("ftp://example.com")).contains("absolute http or https URL"));
        assert_eq!(error(input(MonitorKind::Http, None)), "target is required");
    }

    #[test]
    fn internal_targets_are_rejected_unless_allowed() {
        for target in [
            "http://localhost:8080",
            "http://127.0.0.1",
            "http://10.0.0.5/health",
            "http://[::1]:3000",
            "http://169.254.169.254/latest/meta-data",
        ] {
            assert!(
                error(http(target)).contains("public address"),
                "{target} should be rejected"
            );
            assert!(monitor(http(target), true).is_ok(), "{target} allowed");
        }
    }

    #[test]
    fn expected_status_is_a_status_code_for_http_kinds() {
        let mut ok = http("https://example.com");
        ok.expected_status = Some(204);
        assert_eq!(monitor(ok, false).unwrap().expected_status, Some(204));

        let mut out_of_range = http("https://example.com");
        out_of_range.expected_status = Some(700);
        assert!(error(out_of_range).starts_with("expected_status must be"));

        let mut tcp = input(MonitorKind::Tcp, Some("db.example.com:5432"));
        tcp.expected_status = Some(200);
        assert!(error(tcp).contains("only applies to http and keyword"));
    }

    #[test]
    fn keyword_monitors_need_a_keyword() {
        let missing = input(MonitorKind::Keyword, Some("https://example.com"));
        assert_eq!(error(missing), "keyword is required");

        let mut blank = input(MonitorKind::Keyword, Some("https://example.com"));
        blank.keyword = Some("  ".to_owned());
        assert_eq!(error(blank), "keyword is required");

        let mut ok = input(MonitorKind::Keyword, Some("https://example.com"));
        ok.keyword = Some("Welcome".to_owned());
        assert_eq!(
            monitor(ok, false).unwrap().keyword.as_deref(),
            Some("Welcome")
        );

        let mut misplaced = http("https://example.com");
        misplaced.keyword = Some("Welcome".to_owned());
        assert_eq!(error(misplaced), "keyword only applies to keyword monitors");
    }

    #[test]
    fn tcp_targets_are_host_and_port() {
        let ok = |target: &str| {
            monitor(input(MonitorKind::Tcp, Some(target)), false)
                .unwrap()
                .target
                .unwrap()
        };
        assert_eq!(ok("DB.Example.com:5432"), "db.example.com:5432");
        assert_eq!(ok("1.1.1.1:53"), "1.1.1.1:53");
        assert_eq!(ok("[2606:4700::1111]:443"), "[2606:4700::1111]:443");

        for bad in [
            "db.example.com",
            "db.example.com:0",
            "db.example.com:70000",
            ":5432",
            "https://db.example.com:5432",
            "2606:4700::1111:443",
            "-bad-.example.com:22",
        ] {
            let err = error(input(MonitorKind::Tcp, Some(bad)));
            assert!(err.starts_with("target must be host:port"), "{bad}: {err}");
        }
        assert!(error(input(MonitorKind::Tcp, Some("localhost:5432"))).contains("public"));
    }

    #[test]
    fn heartbeats_have_no_target() {
        let spec = monitor(input(MonitorKind::Heartbeat, None), false).unwrap();
        assert_eq!(spec.target, None);
        assert_eq!(
            error(input(MonitorKind::Heartbeat, Some("https://example.com"))),
            "heartbeat monitors have no target"
        );
    }

    fn channel_input(kind: ChannelKind, config: Value) -> ChannelInput {
        ChannelInput {
            name: " Ops ".to_owned(),
            kind,
            config,
            active: None,
        }
    }

    #[test]
    fn email_channels_need_an_address() {
        let spec = channel(
            &channel_input(ChannelKind::Email, json!({ "to": " ops@example.com " })),
            false,
        )
        .unwrap();
        assert_eq!(spec.name, "Ops");
        assert_eq!(spec.config, json!({ "to": "ops@example.com" }));
        assert!(spec.active);

        for bad in [
            json!({}),
            json!({ "to": "ops" }),
            json!({ "to": "a b@c.d" }),
            json!({ "to": "a@b" }),
        ] {
            assert!(
                channel(&channel_input(ChannelKind::Email, bad.clone()), false).is_err(),
                "{bad}"
            );
        }
    }

    #[test]
    fn webhook_channels_need_a_public_url() {
        let spec = channel(
            &channel_input(
                ChannelKind::Discord,
                json!({ "url": "https://discord.com/api/webhooks/1/abc", "extra": 1 }),
            ),
            false,
        )
        .unwrap();
        assert_eq!(
            spec.config,
            json!({ "url": "https://discord.com/api/webhooks/1/abc" }),
            "unknown keys are dropped"
        );

        let err = channel(
            &channel_input(
                ChannelKind::Webhook,
                json!({ "url": "http://10.0.0.1/hook" }),
            ),
            false,
        )
        .unwrap_err();
        assert!(
            err.starts_with("config.url must be a public address"),
            "{err}"
        );
        assert!(
            channel(
                &channel_input(ChannelKind::Slack, json!({ "url": "ftp://x" })),
                false
            )
            .is_err()
        );
    }
}

//! What an alert says, per channel kind. Pure; user-facing text is in Spanish.

use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use taut_model::events::IncidentChanged;

/// An alert rendered for every channel kind.
#[derive(Debug, Clone, PartialEq)]
pub struct Rendered {
    /// Email subject.
    pub subject: String,
    /// Email body, plain text.
    pub text: String,
    /// Discord incoming webhook body.
    pub discord: Value,
    /// Slack incoming webhook body.
    pub slack: Value,
    /// Generic webhook body.
    pub webhook: Value,
}

/// Renders an incident transition. `panel_url` is the base URL of the panel.
pub fn incident(event: &IncidentChanged, panel_url: &str) -> Rendered {
    let link = format!(
        "{}/monitors/{}",
        panel_url.trim_end_matches('/'),
        event.monitor_id
    );
    let name = &event.monitor_name;
    let started = format_time(event.started_at);
    match event.resolved_at {
        None => Rendered {
            subject: format!("[taut] Caído: {name}"),
            text: format!(
                "El monitor «{name}» está caído desde el {started}.\n\n\
                 Causa: {cause}\n\n\
                 Detalle: {link}\n",
                cause = event.cause
            ),
            discord: json!({
                "content": format!("🔴 **{name}** está caído: {}\n{link}", event.cause)
            }),
            slack: json!({
                "text": format!("🔴 *{name}* está caído: {}\n<{link}|Ver en taut>", event.cause)
            }),
            webhook: webhook_body("incident.opened", event, &link),
        },
        Some(resolved_at) => {
            let duration = format_duration((resolved_at - event.started_at).num_seconds());
            Rendered {
                subject: format!("[taut] Recuperado: {name}"),
                text: format!(
                    "El monitor «{name}» se recuperó el {resolved} después de {duration} caído.\n\n\
                     Causa original: {cause}\n\n\
                     Detalle: {link}\n",
                    resolved = format_time(resolved_at),
                    cause = event.cause
                ),
                discord: json!({
                    "content": format!("🟢 **{name}** se recuperó tras {duration}.\n{link}")
                }),
                slack: json!({
                    "text": format!("🟢 *{name}* se recuperó tras {duration}.\n<{link}|Ver en taut>")
                }),
                webhook: webhook_body("incident.resolved", event, &link),
            }
        }
    }
}

/// Renders the message of the "send test" button.
pub fn test_message(channel_name: &str) -> Rendered {
    let text = format!(
        "Mensaje de prueba del canal «{channel_name}» de taut. Si lo ves, las alertas llegarán aquí."
    );
    Rendered {
        subject: "[taut] Prueba de alertas".to_owned(),
        text: format!("{text}\n"),
        discord: json!({ "content": format!("🔔 {text}") }),
        slack: json!({ "text": format!("🔔 {text}") }),
        webhook: json!({ "event": "test", "channel": channel_name, "message": text }),
    }
}

fn webhook_body(event_name: &str, event: &IncidentChanged, link: &str) -> Value {
    json!({
        "event": event_name,
        "incident": {
            "id": event.incident_id,
            "started_at": event.started_at,
            "resolved_at": event.resolved_at,
            "cause": event.cause,
            "duration_s": event.resolved_at.map(|resolved| (resolved - event.started_at).num_seconds()),
        },
        "monitor": { "id": event.monitor_id, "name": event.monitor_name },
        "tenant_id": event.tenant_id,
        "url": link,
    })
}

/// `16/10/2026 04:18 UTC`.
fn format_time(time: DateTime<Utc>) -> String {
    time.format("%d/%m/%Y %H:%M UTC").to_string()
}

/// `45 s`, `3 min 20 s`, `2 h 5 min`.
pub fn format_duration(seconds: i64) -> String {
    let seconds = seconds.max(0);
    match seconds {
        0..60 => format!("{seconds} s"),
        60..3600 => {
            let rest = seconds % 60;
            if rest == 0 {
                format!("{} min", seconds / 60)
            } else {
                format!("{} min {rest} s", seconds / 60)
            }
        }
        _ => {
            let minutes = (seconds % 3600) / 60;
            if minutes == 0 {
                format!("{} h", seconds / 3600)
            } else {
                format!("{} h {minutes} min", seconds / 3600)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use super::*;

    fn event(resolved_after_s: Option<i64>) -> IncidentChanged {
        let started_at = DateTime::from_timestamp(1_792_124_280, 0).unwrap();
        IncidentChanged {
            trace_id: "trace".to_owned(),
            incident_id: Uuid::nil(),
            tenant_id: Uuid::nil(),
            monitor_id: Uuid::nil(),
            monitor_name: "API".to_owned(),
            started_at,
            resolved_at: resolved_after_s.map(|s| started_at + chrono::Duration::seconds(s)),
            cause: "status 503 (expected 2xx)".to_owned(),
        }
    }

    #[test]
    fn opened_alert_names_monitor_cause_and_link() {
        let rendered = incident(&event(None), "http://panel.test/");
        assert_eq!(rendered.subject, "[taut] Caído: API");
        assert!(rendered.text.contains("Causa: status 503 (expected 2xx)"));
        assert!(
            rendered
                .text
                .contains("http://panel.test/monitors/00000000-0000-0000-0000-000000000000")
        );
        assert!(
            rendered.discord["content"]
                .as_str()
                .unwrap()
                .starts_with("🔴 **API**")
        );
        assert_eq!(rendered.webhook["event"], "incident.opened");
        assert_eq!(rendered.webhook["incident"]["duration_s"], Value::Null);
    }

    #[test]
    fn resolved_alert_says_how_long_it_was_down() {
        let rendered = incident(&event(Some(200)), "http://panel.test");
        assert_eq!(rendered.subject, "[taut] Recuperado: API");
        assert!(rendered.text.contains("después de 3 min 20 s caído"));
        assert!(
            rendered.slack["text"]
                .as_str()
                .unwrap()
                .contains("tras 3 min 20 s")
        );
        assert_eq!(rendered.webhook["event"], "incident.resolved");
        assert_eq!(rendered.webhook["incident"]["duration_s"], 200);
    }

    #[test]
    fn durations_use_the_largest_units() {
        assert_eq!(format_duration(45), "45 s");
        assert_eq!(format_duration(120), "2 min");
        assert_eq!(format_duration(7500), "2 h 5 min");
        assert_eq!(format_duration(-3), "0 s");
    }
}

//! Account emails: built here, sent by the notifier.

use taut_bus::Bus;
use taut_http::{AppError, current_trace_id};
use taut_model::events::AccountEmail;
use ulid::Ulid;

/// Trace id of the current request, or a fresh one.
pub fn trace_id() -> String {
    current_trace_id().unwrap_or_else(|| Ulid::generate().to_string())
}

/// Publishes `email` once per `message_id` (the token id), so a retried request cannot send
/// the same link twice.
pub async fn publish(bus: &Bus, message_id: &str, email: &AccountEmail) -> Result<(), AppError> {
    bus.publish_once(email.subject(), email.trace_id(), message_id, email)
        .await
        .map_err(|err| AppError::Internal(anyhow::anyhow!("publish account email: {err}")))?;
    metrics::counter!("auth_emails_published_total", "subject" => email.subject()).increment(1);
    tracing::info!(subject = email.subject(), "account email published");
    Ok(())
}

/// Panel URL of the page that completes a flow, with the secret as a query parameter.
pub fn link(panel_url: &str, page: &str, token: &str) -> String {
    format!("{}/{page}?token={token}", panel_url.trim_end_matches('/'))
}

/// Panel URL of a page without a token.
pub fn page(panel_url: &str, page: &str) -> String {
    format!("{}/{page}", panel_url.trim_end_matches('/'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn links_join_the_panel_url_and_the_token() {
        assert_eq!(
            link("http://panel.test/", "verify-email", "abc"),
            "http://panel.test/verify-email?token=abc"
        );
        assert_eq!(
            page("http://panel.test", "login"),
            "http://panel.test/login"
        );
    }
}

//! Transactional email delivery policies.
//!
//! The SMTP service owns I/O, but this module owns user-visible and operator
//! error contracts for email configuration, recipient validation, and send
//! failures.

use agentforge_core::{AppError, ErrorKind};

pub(crate) struct EmailDeliveryPolicy;

impl EmailDeliveryPolicy {
    pub(crate) fn smtp_not_configured() -> AppError {
        ErrorKind::Internal(anyhow::anyhow!("SMTP is not configured")).into()
    }

    pub(crate) fn required_when_host(name: &str) -> AppError {
        ErrorKind::Internal(anyhow::anyhow!("{name} is required when SMTP_HOST is set")).into()
    }

    pub(crate) fn invalid_relay(host: &str, err: impl std::fmt::Display) -> AppError {
        ErrorKind::Internal(anyhow::anyhow!("invalid SMTP relay {host}: {err}")).into()
    }

    pub(crate) fn invalid_from_mailbox(err: impl std::fmt::Display) -> AppError {
        ErrorKind::Internal(anyhow::anyhow!("invalid SMTP_FROM mailbox: {err}")).into()
    }

    pub(crate) fn invalid_recipient(err: impl std::fmt::Display) -> ErrorKind {
        ErrorKind::Validation(format!("invalid recipient email: {err}"))
    }

    pub(crate) fn build_message_failed(err: impl std::fmt::Display) -> ErrorKind {
        ErrorKind::Internal(anyhow::anyhow!("build email message: {err}"))
    }

    pub(crate) fn send_failed(err: impl std::fmt::Display) -> ErrorKind {
        ErrorKind::Internal(anyhow::anyhow!("send email through SMTP: {err}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_internal_message(err: AppError, expected: &str) {
        match err.kind {
            ErrorKind::Internal(message) => assert!(message.to_string().contains(expected)),
            other => panic!("expected internal error, got {other:?}"),
        }
    }

    #[test]
    fn email_delivery_policy_owns_smtp_error_contracts() {
        assert_internal_message(EmailDeliveryPolicy::smtp_not_configured(), "SMTP is not configured");
        assert_internal_message(EmailDeliveryPolicy::required_when_host("SMTP_USER"), "SMTP_USER is required");
        assert_internal_message(EmailDeliveryPolicy::invalid_relay("smtp.example.com", "bad"), "invalid SMTP relay");
        assert_internal_message(EmailDeliveryPolicy::invalid_from_mailbox("bad"), "invalid SMTP_FROM mailbox");
        assert!(format!("{}", EmailDeliveryPolicy::invalid_recipient("bad")).contains("invalid recipient email"));
        assert!(format!("{}", EmailDeliveryPolicy::build_message_failed("bad")).contains("build email message"));
        assert!(format!("{}", EmailDeliveryPolicy::send_failed("bad")).contains("send email through SMTP"));
    }
}

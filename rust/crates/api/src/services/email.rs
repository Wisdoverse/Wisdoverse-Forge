//! Transactional email delivery.
//!
//! Auth flows depend on this for password reset. The disabled sender is
//! explicit so production does not silently accept reset requests when SMTP is
//! not configured.

use std::str::FromStr;
use std::sync::Arc;

use agentforge_core::{AppConfig, AppError, AppResult, ErrorKind};
use async_trait::async_trait;
use lettre::message::Mailbox;
use lettre::transport::smtp::authentication::Credentials;
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};
use secrecy::ExposeSecret;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmailMessage {
    pub to: String,
    pub subject: String,
    pub body: String,
}

#[async_trait]
pub trait EmailSender: Send + Sync {
    fn is_configured(&self) -> bool;
    async fn send(&self, message: EmailMessage) -> AppResult<()>;
}

#[derive(Debug, Default)]
pub struct DisabledEmailSender;

#[async_trait]
impl EmailSender for DisabledEmailSender {
    fn is_configured(&self) -> bool {
        false
    }

    async fn send(&self, _message: EmailMessage) -> AppResult<()> {
        Err(ErrorKind::Internal(anyhow::anyhow!("SMTP is not configured")).into())
    }
}

pub struct SmtpEmailSender {
    from: Mailbox,
    transport: AsyncSmtpTransport<Tokio1Executor>,
}

impl SmtpEmailSender {
    pub fn from_config(config: &AppConfig) -> AppResult<Option<Self>> {
        let Some(host) = config.smtp_host.as_deref().map(str::trim).filter(|value| !value.is_empty()) else {
            return Ok(None);
        };
        let user = required_trimmed(config.smtp_user.as_deref(), "SMTP_USER")?;
        let password = config
            .smtp_password
            .as_ref()
            .map(|secret| secret.expose_secret().trim())
            .filter(|value| !value.is_empty())
            .ok_or_else(|| internal_config_error("SMTP_PASSWORD is required when SMTP_HOST is set"))?;
        let from = config
            .smtp_from
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| internal_config_error("SMTP_FROM is required when SMTP_HOST is set"))?;

        let credentials = Credentials::new(user.to_string(), password.to_string());
        let mut builder = if config.smtp_secure {
            AsyncSmtpTransport::<Tokio1Executor>::relay(host)
                .map_err(|err| internal_config_error(format!("invalid SMTP relay {host}: {err}")))?
        } else {
            AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(host)
        };
        if let Some(port) = config.smtp_port {
            builder = builder.port(port);
        }

        Ok(Some(Self {
            from: Mailbox::from_str(from)
                .map_err(|err| internal_config_error(format!("invalid SMTP_FROM mailbox: {err}")))?,
            transport: builder.credentials(credentials).build(),
        }))
    }
}

#[async_trait]
impl EmailSender for SmtpEmailSender {
    fn is_configured(&self) -> bool {
        true
    }

    async fn send(&self, message: EmailMessage) -> AppResult<()> {
        let email = Message::builder()
            .from(self.from.clone())
            .to(Mailbox::from_str(&message.to)
                .map_err(|err| ErrorKind::Validation(format!("invalid recipient email: {err}")))?)
            .subject(message.subject)
            .body(message.body)
            .map_err(|err| ErrorKind::Internal(anyhow::anyhow!("build email message: {err}")))?;

        self.transport
            .send(email)
            .await
            .map_err(|err| ErrorKind::Internal(anyhow::anyhow!("send email through SMTP: {err}")))?;
        Ok(())
    }
}

pub fn sender_from_config(config: &AppConfig) -> AppResult<Arc<dyn EmailSender>> {
    match SmtpEmailSender::from_config(config)? {
        Some(sender) => Ok(Arc::new(sender)),
        None => Ok(Arc::new(DisabledEmailSender)),
    }
}

fn required_trimmed<'a>(value: Option<&'a str>, name: &str) -> AppResult<&'a str> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| internal_config_error(format!("{name} is required when SMTP_HOST is set")))
}

fn internal_config_error(message: impl Into<String>) -> AppError {
    ErrorKind::Internal(anyhow::anyhow!(message.into())).into()
}

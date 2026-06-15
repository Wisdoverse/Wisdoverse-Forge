//! NATS connection for the event pipeline and WebSocket broadcast.
//!
//! NATS replaces Redis PubSub for cross-instance event broadcasting.
//! The Rust sidecar publishes to NATS JetStream; the Rust backend consumes from it.

use agentforge_core::{AppConfig, AppResult, ErrorKind};
use serde_json::Value;
use url::{Host, Url};

#[derive(Debug, Clone, PartialEq, Eq)]
enum NatsAuth {
    None,
    Token(String),
    UserPassword { user: String, password: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct NatsConnectionTarget {
    server_url: String,
    auth: NatsAuth,
}

fn prepare_connection_target(url: &str) -> Option<NatsConnectionTarget> {
    let parsed = Url::parse(url).ok()?;
    let host = parsed.host()?;
    let authority = match host {
        Host::Domain(domain) => domain.to_string(),
        Host::Ipv4(addr) => addr.to_string(),
        Host::Ipv6(addr) => format!("[{addr}]"),
    };

    let mut server_url = format!("{}://{}", parsed.scheme(), authority);
    if let Some(port) = parsed.port() {
        server_url.push(':');
        server_url.push_str(&port.to_string());
    }
    if parsed.path() != "/" {
        server_url.push_str(parsed.path());
    }
    if let Some(query) = parsed.query() {
        server_url.push('?');
        server_url.push_str(query);
    }
    if let Some(fragment) = parsed.fragment() {
        server_url.push('#');
        server_url.push_str(fragment);
    }

    let auth = match (parsed.username(), parsed.password()) {
        ("", Some(token)) => NatsAuth::Token(token.to_string()),
        (token, None) if !token.is_empty() => NatsAuth::Token(token.to_string()),
        (user, Some(password)) if !user.is_empty() => {
            NatsAuth::UserPassword { user: user.to_string(), password: password.to_string() }
        }
        _ => NatsAuth::None,
    };

    Some(NatsConnectionTarget { server_url, auth })
}

/// Surface asynchronous NATS connection events as logs.
///
/// `client.publish(...)` on a core-NATS subject is fire-and-forget: a
/// server-side rejection (e.g. a Permissions Violation when a connection
/// publishes to a subject its JWT did not grant) is delivered out-of-band to
/// this callback, NOT as a `publish()` error. Without it such a rejection is
/// invisible. This matters for the #457 event-ingest namespacing: if a
/// sidecar's published `events.ingest.<kind>.<uuid>` ever diverges from the
/// kind the auth callout granted, every event would be dropped server-side
/// with no local signal. Logging the server error makes that condition
/// debuggable instead of silent.
async fn log_nats_event(event: async_nats::Event) {
    match event {
        async_nats::Event::ServerError(err) => {
            tracing::error!(%err, "NATS server error (a publish/subscribe may have been rejected — e.g. a subject not granted to this connection)");
        }
        async_nats::Event::ClientError(err) => {
            tracing::warn!(%err, "NATS client error");
        }
        async_nats::Event::Disconnected => tracing::warn!("NATS disconnected"),
        async_nats::Event::Connected => tracing::info!("NATS (re)connected"),
        other => tracing::debug!(event = ?other, "NATS connection event"),
    }
}

pub async fn connect_nats(url: &str) -> Result<async_nats::Client, async_nats::ConnectError> {
    let Some(target) = prepare_connection_target(url) else {
        return async_nats::ConnectOptions::new().event_callback(log_nats_event).connect(url).await;
    };

    match target.auth {
        NatsAuth::None => {
            async_nats::ConnectOptions::new().event_callback(log_nats_event).connect(target.server_url).await
        }
        NatsAuth::Token(token) => {
            async_nats::ConnectOptions::with_token(token)
                .event_callback(log_nats_event)
                .connect(target.server_url)
                .await
        }
        NatsAuth::UserPassword { user, password } => {
            async_nats::ConnectOptions::with_user_and_password(user, password)
                .event_callback(log_nats_event)
                .connect(target.server_url)
                .await
        }
    }
}

/// NATS client wrapper with connection state tracking.
pub struct NatsClient {
    client: Option<async_nats::Client>,
}

impl NatsClient {
    /// Create a NATS client. Returns successfully even if NATS is unavailable.
    pub async fn new(config: &AppConfig) -> Self {
        let Some(url) = &config.nats_url else {
            tracing::info!("NATS URL not configured, running without NATS");
            return Self { client: None };
        };

        match connect_nats(url).await {
            Ok(client) => {
                tracing::info!("NATS connected");
                Self { client: Some(client) }
            }
            Err(err) => {
                tracing::warn!(error = %err, "NATS connection failed, running without NATS");
                Self { client: None }
            }
        }
    }

    /// Returns `true` if a NATS client is available.
    pub fn is_connected(&self) -> bool {
        self.client.is_some()
    }

    /// Get a reference to the underlying `async_nats::Client`.
    pub fn client(&self) -> Option<&async_nats::Client> {
        self.client.as_ref()
    }

    /// Publish a JSON payload to a NATS subject.
    pub async fn publish_json(&self, subject: &str, payload: Value) -> AppResult<()> {
        let client = self
            .client
            .as_ref()
            .ok_or_else(|| ErrorKind::Internal(std::io::Error::other("NATS not connected").into()))?;

        let bytes = serde_json::to_vec(&payload).map_err(|err| {
            ErrorKind::Internal(std::io::Error::other(format!("failed to serialize NATS payload: {err}")).into())
        })?;

        client.publish(subject.to_string(), bytes.into()).await.map_err(|err| {
            ErrorKind::Internal(std::io::Error::other(format!("failed to publish NATS message: {err}")).into())
        })?;
        Ok(())
    }

    /// Health check — returns `true` if the NATS connection is in `Connected` state.
    pub async fn check_health(&self) -> bool {
        match &self.client {
            Some(client) => client.connection_state() == async_nats::connection::State::Connected,
            None => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_jwt_secret() -> secrecy::SecretString {
        secrecy::SecretString::from("agentforge-jwt-placeholder-for-tests".to_string())
    }

    fn test_config(nats_url: Option<String>) -> AppConfig {
        AppConfig {
            port: 4003,
            host: "0.0.0.0".to_string(),
            database_url: "postgres://localhost/test".to_string(),
            redis_url: None,
            presence_redis_enabled: false,
            nats_url,
            nats_agent_url: None,
            nats_container_url: None,
            nats_callout: agentforge_core::NatsCalloutConfig::default(),
            stripe: agentforge_core::StripeConfig::default(),
            jwt_secret: test_jwt_secret(),
            jwt_expiry_seconds: 900,
            environment: "development".to_string(),
            log_level: "info".to_string(),
            cors_origin: None,
            static_dir: None,
            container_server_url: None,
            ollama_base_url: None,
            llm_encryption_key: None,
            container_anthropic_api_key: None,
            container_google_api_key: None,
            container_openai_api_key: None,
            codex_default_model: "gpt-5.5".to_string(),
            oauth_mount_dir: None,
            storage_provider: "local".to_string(),
            storage_local_path: "~/.agentforge/data/uploads".to_string(),
            storage_max_file_size: 10 * 1024 * 1024,
            storage_max_files_per_session: 20,
            storage_signed_url_expiry: 3600,
            minio_endpoint: None,
            minio_access_key: None,
            minio_secret_key: None,
            minio_bucket: "agentforge".to_string(),
            minio_use_ssl: false,
            minio_region: None,
            credential_sync_enabled: false,
            cli_auth_proxy_openai_client_id: None,
            cli_auth_proxy_openai_client_secret: None,
            cli_auth_proxy_openai_auth_endpoint: None,
            cli_auth_proxy_openai_token_endpoint: None,
            app_url: None,
            cli_auth_proxy_revoke_threshold: 2,
            smtp_host: None,
            smtp_port: None,
            smtp_user: None,
            smtp_password: None,
            smtp_from: None,
            smtp_secure: false,
            allow_plaintext_host_nats: false,
            host_join_binary_base_url: None,
            cli_image_auto_update_enabled: false,
            cli_image_auto_update_interval_secs: 900,
            cli_image_prune_enabled: false,
            cli_image_claude_auto_build: false,
            cli_image_npm_registry: None,
            project_clone_worker_enabled: false,
            project_clone_image: None,
            project_clone_secret_root: None,
            project_clone_timeout_secs: 600,
        }
    }

    #[tokio::test]
    async fn no_url_configured_not_connected() {
        let client = NatsClient::new(&test_config(None)).await;
        assert!(!client.is_connected());
        assert!(client.client().is_none());
    }

    #[tokio::test]
    async fn health_check_false_when_not_connected() {
        let client = NatsClient::new(&test_config(None)).await;
        assert!(!client.check_health().await);
    }

    #[tokio::test]
    async fn invalid_url_degrades_gracefully() {
        // An unreachable NATS URL should not panic — just degrade.
        let client = NatsClient::new(&test_config(Some("nats://127.0.0.1:1".to_string()))).await;
        assert!(!client.is_connected());
    }

    #[test]
    fn token_in_username_is_treated_as_nats_token_auth() {
        let target = prepare_connection_target("nats://secret-token@nats:4222").expect("parse target");
        assert_eq!(target.server_url, "nats://nats:4222");
        assert_eq!(target.auth, NatsAuth::Token("secret-token".to_string()));
    }

    #[test]
    fn token_in_password_is_treated_as_nats_token_auth() {
        let nats_url = ["nats://:", "secret-token", "@nats:4222"].concat();
        let target = prepare_connection_target(&nats_url).expect("parse target");
        assert_eq!(target.server_url, "nats://nats:4222");
        assert_eq!(target.auth, NatsAuth::Token("secret-token".to_string()));
    }

    #[test]
    fn username_password_auth_is_preserved() {
        let nats_url = ["nats://alice:", "secret", "@nats:4222"].concat();
        let target = prepare_connection_target(&nats_url).expect("parse target");
        assert_eq!(target.server_url, "nats://nats:4222");
        assert_eq!(target.auth, NatsAuth::UserPassword { user: "alice".to_string(), password: "secret".to_string() });
    }
}

//! Redis connection with graceful degradation.
//!
//! Redis is **optional** in Wisdoverse Forge (see CLAUDE.md: "Circuit breaker: Redis is optional").
//! When Redis is not configured or unreachable, the client returns `None` and the
//! application continues without Redis-backed features (caching, PubSub, etc.).

use agentforge_core::AppConfig;
use redis::Client;
use redis::aio::MultiplexedConnection;

/// Optional Redis connection — `None` if not configured or connection fails.
pub struct RedisClient {
    connection: Option<MultiplexedConnection>,
}

impl RedisClient {
    /// Create a Redis client. Returns `Ok` even if Redis is unavailable (graceful degradation).
    pub async fn new(config: &AppConfig) -> Self {
        let Some(url) = &config.redis_url else {
            tracing::info!("Redis URL not configured, running without Redis");
            return Self { connection: None };
        };

        match Client::open(url.as_str()) {
            Ok(client) => match client.get_multiplexed_async_connection().await {
                Ok(conn) => {
                    tracing::info!("Redis connected");
                    Self { connection: Some(conn) }
                }
                Err(err) => {
                    tracing::warn!(error = %err, "Redis connection failed, running without Redis");
                    Self { connection: None }
                }
            },
            Err(err) => {
                tracing::warn!(error = %err, "Redis client creation failed, running without Redis");
                Self { connection: None }
            }
        }
    }

    /// Returns `true` if a Redis connection is available.
    pub fn is_connected(&self) -> bool {
        self.connection.is_some()
    }

    /// Get a reference to the connection for Redis operations.
    /// Returns `None` if Redis is unavailable.
    pub fn connection(&self) -> Option<&MultiplexedConnection> {
        self.connection.as_ref()
    }

    /// Get a mutable reference to the connection (needed for `redis::cmd` queries).
    pub fn connection_mut(&mut self) -> Option<&mut MultiplexedConnection> {
        self.connection.as_mut()
    }

    /// Health check — sends a PING command and returns `true` if Redis responds.
    pub async fn check_health(&mut self) -> bool {
        let Some(conn) = &mut self.connection else {
            return false;
        };
        redis::cmd("PING").query_async::<String>(conn).await.is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_jwt_secret() -> secrecy::SecretString {
        secrecy::SecretString::from("agentforge-jwt-placeholder-for-tests".to_string())
    }

    fn test_config(redis_url: Option<String>) -> AppConfig {
        AppConfig {
            port: 4003,
            host: "0.0.0.0".to_string(),
            database_url: "postgres://localhost/test".to_string(),
            redis_url,
            nats_url: None,
            nats_agent_url: None,
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
            cli_image_auto_update_enabled: false,
            cli_image_auto_update_interval_secs: 900,
        }
    }

    #[tokio::test]
    async fn no_url_configured_not_connected() {
        let client = RedisClient::new(&test_config(None)).await;
        assert!(!client.is_connected());
        assert!(client.connection().is_none());
    }

    #[tokio::test]
    async fn health_check_false_when_not_connected() {
        let mut client = RedisClient::new(&test_config(None)).await;
        assert!(!client.check_health().await);
    }

    #[tokio::test]
    async fn invalid_url_degrades_gracefully() {
        // An unreachable URL should not panic — just degrade.
        let client = RedisClient::new(&test_config(Some("redis://127.0.0.1:1".to_string()))).await;
        assert!(!client.is_connected());
    }
}

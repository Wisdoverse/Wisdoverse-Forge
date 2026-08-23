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
        Self::connect(config.redis_url.as_deref()).await
    }

    /// Connect from an explicit URL (or `None` to run without Redis). Same
    /// graceful-degradation contract as [`RedisClient::new`]; kept public so
    /// integration tests can build a client without a full `AppConfig`.
    pub async fn connect(url: Option<&str>) -> Self {
        let Some(url) = url else {
            tracing::info!("Redis URL not configured, running without Redis");
            return Self { connection: None };
        };

        match Client::open(url) {
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

    /// Probe the actual read/write path used by the OAuth/PKCE state store:
    /// `SET` a short-TTL throwaway key, then `GETDEL` it (the exact operations
    /// `cli_auth_proxy` performs). Unlike [`is_connected`](Self::is_connected)
    /// (socket opened) or [`check_health`](Self::check_health) (PING only), this
    /// verifies the connection can actually WRITE — so a reachable but read-only
    /// or ACL-restricted Redis (connects, rejects `SET`) is detected. Returns
    /// `false` if Redis is absent or any step fails.
    pub async fn probe_read_write(&mut self) -> bool {
        use redis::AsyncCommands;
        use std::sync::OnceLock;
        use std::sync::atomic::{AtomicU64, Ordering};

        let Some(conn) = &mut self.connection else {
            return false;
        };
        // Probe the SAME keyspace the OAuth state store uses
        // (`cli-auth-proxy:state:`), so a Redis ACL that restricts that key
        // pattern is tested accurately — probing a different prefix could pass or
        // fail independently of the path actually being guarded. A distinctive
        // `__rw-probe__` marker keeps it from ever colliding with a real state key.
        //
        // The instance id is a per-PROCESS random UUID, NOT `process::id()`:
        // containers commonly share pid 1, so pid + a per-process counter (which
        // also starts at 0 everywhere) would NOT be unique across replicas, and one
        // replica's `GETDEL` could consume another's key and falsely fail a healthy
        // Redis. UUID + monotonic seq makes every probe key distinct across
        // replicas and across repeated (readiness) probes within a process.
        static INSTANCE: OnceLock<String> = OnceLock::new();
        static PROBE_SEQ: AtomicU64 = AtomicU64::new(0);
        let instance = INSTANCE.get_or_init(|| uuid::Uuid::new_v4().to_string());
        let key =
            format!("cli-auth-proxy:state:__rw-probe__:{}:{}", instance, PROBE_SEQ.fetch_add(1, Ordering::Relaxed));
        if conn.set_ex::<_, _, ()>(&key, "1", 10).await.is_err() {
            return false;
        }
        let got: redis::RedisResult<Option<String>> = redis::cmd("GETDEL").arg(&key).query_async(conn).await;
        matches!(got, Ok(Some(_)))
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
            presence_redis_enabled: false,
            require_external_state: false,
            nats_url: None,
            nats_agent_url: None,
            nats_container_url: None,
            nats_callout: agentforge_core::NatsCalloutConfig::default(),
            stripe: agentforge_core::StripeConfig::default(),
            jwt_secret: test_jwt_secret(),
            bootstrap_admin_token: None,
            allow_unprotected_admin_bootstrap: false,
            jwt_expiry_seconds: 900,
            environment: "development".to_string(),
            log_level: "info".to_string(),
            cors_origin: None,
            static_dir: None,
            container_server_url: None,
            ollama_base_url: None,
            dev_env_allowed_image_registries: Vec::new(),
            force_reset_legacy_sha256: false,
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
            github_app_id: None,
            github_app_installation_id: None,
            github_app_private_key: None,
            github_app_repo: None,
            self_fix_pr_worker_enabled: false,
            self_fix_max_merge_attempts: 5,
            self_fix_review_deadline_secs: 604800,
            blocked_task_ttl_secs: 3600,
            job_queue_stale_lock_timeout_secs: 1800,
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
    async fn probe_read_write_false_when_not_connected() {
        // No connection → the CN-7 startup probe must report Redis unusable, so
        // ensure_external_state_redis_ready fails fast under REQUIRE_EXTERNAL_STATE.
        let mut client = RedisClient::new(&test_config(None)).await;
        assert!(!client.probe_read_write().await);
    }

    #[tokio::test]
    async fn invalid_url_degrades_gracefully() {
        // An unreachable URL should not panic — just degrade.
        let client = RedisClient::new(&test_config(Some("redis://127.0.0.1:1".to_string()))).await;
        assert!(!client.is_connected());
    }
}

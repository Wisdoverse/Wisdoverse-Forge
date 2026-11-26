//! Redis connection with graceful degradation and automatic recovery.
//!
//! Redis is **optional** in Wisdoverse Forge (see CLAUDE.md: "Circuit breaker: Redis is optional").
//! When Redis is not configured or not usable, [`RedisClient::handle`] returns `None` and the
//! application continues without Redis-backed features (caching, shared OAuth state, etc.).
//!
//! # Recovery
//!
//! With a URL configured the client keeps an explicit state, usable or down:
//!
//! - Commands go through the `redis` crate's [`ConnectionManager`], built lazily with the
//!   crate's own reconnection retries switched off. While the manager reconnects, commands
//!   issued after a failure wait for that reconnection outside the response timeout
//!   (`redis-1.8.2/src/aio/connection_manager.rs:380-382`, `:702-706`), so request paths
//!   never use it while the state is down: [`RedisClient::handle`] returns `None` at once.
//! - Every request-path command runs under one overall deadline
//!   (`Timings::DEFAULT.request_deadline`, 1 s). A deadline or connection error marks the
//!   state down immediately (the next caller degrades without waiting) and wakes the monitor.
//! - One background monitor task per client owns recovery. While usable it PINGs every 5 s,
//!   so a loss is noticed even when nothing uses Redis; while down it PINGs with capped
//!   exponential backoff (250 ms doubling to 5 s, with jitter) until Redis answers, forever.
//!   It ends when the client is dropped. Being unreachable at startup is the same case as
//!   losing the connection later.
//! - Redis is used again at most `Timings::recovery_bound` (9.5 s) after it is reachable.
//! - Commands are never replayed. A command that timed out may still have run on the server
//!   (for example a `GETDEL` that consumed OAuth state), so callers must keep treating a
//!   timeout as a failure of that operation.
//! - Each change of state logs one line (`warn` when lost, `info` with the outage length when
//!   restored) and counts `agentforge_redis_state_changes_total{state}`.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use agentforge_core::AppConfig;
use redis::aio::{ConnectionLike, ConnectionManager, ConnectionManagerConfig};
use redis::{Client, Cmd, Pipeline, RedisError, RedisFuture, RedisResult, Value};
use tokio::sync::Notify;

/// The numbers behind connection handling and recovery.
#[derive(Clone, Copy, Debug)]
struct Timings {
    /// One connection attempt, TCP connect plus handshake (the crate's default, now explicit).
    connect_timeout: Duration,
    /// One reply on an open connection (the crate's default, now explicit).
    response_timeout: Duration,
    /// Everything one request-path command may take, including a reconnection it lands on.
    request_deadline: Duration,
    /// PING interval while usable.
    probe_interval: Duration,
    /// First retry delay while down; doubles per failed probe up to `backoff_max`.
    backoff_min: Duration,
    backoff_max: Duration,
}

impl Timings {
    /// Production values; `docs/guides/configuration.md` quotes them. The request deadline
    /// covers a full response timeout with headroom, keeps a command that lands on a
    /// reconnection near one connection timeout, and keeps the readiness write probe (two
    /// commands) within its 2 s budget.
    const DEFAULT: Self = Self {
        connect_timeout: Duration::from_secs(1),
        response_timeout: Duration::from_millis(500),
        request_deadline: Duration::from_secs(1),
        probe_interval: Duration::from_secs(5),
        backoff_min: Duration::from_millis(250),
        backoff_max: Duration::from_secs(5),
    };

    /// A monitor PING may first have to wait for a connection attempt, then for its reply.
    fn probe_deadline(&self) -> Duration {
        self.connect_timeout + self.response_timeout
    }

    /// Longest time from "Redis is reachable" to "the state is usable": a probe already
    /// running may still fail (two PINGs), the monitor then waits at most one interval or
    /// backoff step, and the next probe succeeds within one PING deadline.
    #[cfg(test)]
    fn recovery_bound(&self) -> Duration {
        self.probe_deadline() * 3 + self.probe_interval.max(self.backoff_max)
    }

    /// Delay before the next probe after `failures` failed probes while down. Equal jitter
    /// (half fixed, half random) spreads replicas out and never exceeds `backoff_max`.
    fn backoff(&self, failures: u32) -> Duration {
        let ceiling = self.backoff_min.saturating_mul(1 << failures.min(16)).min(self.backoff_max);
        let random = (uuid::Uuid::new_v4().as_u128() % 1024) as u32;
        ceiling / 2 + ceiling / 2 * random / 1024
    }
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// An I/O failure (refused, dropped, timed out) or one the crate says needs a new connection.
/// A server error reply (`READONLY`, `NOPERM`, ...) is not: the connection works.
fn is_connection_failure(err: &RedisError) -> bool {
    err.is_io_error() || err.is_unrecoverable_error()
}

/// Shared usable/down state of one client.
struct Health {
    usable: AtomicBool,
    /// Start of the current outage; `None` while usable.
    down_since: Mutex<Option<Instant>>,
    /// Wakes the monitor when a request-path command finds Redis gone.
    wake: Notify,
}

impl Health {
    fn mark_down(&self, err: &RedisError) {
        if self.usable.swap(false, Ordering::AcqRel) {
            *lock(&self.down_since) = Some(Instant::now());
            metrics::counter!("agentforge_redis_state_changes_total", "state" => "lost").increment(1);
            tracing::warn!(error = %err, "Redis connection lost; Redis-backed features are degraded until it is restored");
            self.wake.notify_one();
        }
    }

    fn mark_up(&self) {
        if !self.usable.swap(true, Ordering::AcqRel) {
            let down_for = lock(&self.down_since).take().map(|since| since.elapsed()).unwrap_or_default();
            metrics::counter!("agentforge_redis_state_changes_total", "state" => "restored").increment(1);
            tracing::info!(down_for_ms = down_for.as_millis() as u64, "Redis connection restored");
        }
    }
}

/// An owned, cheap Redis connection handle from [`RedisClient::handle`]. Take it under a short
/// lock on the client and run commands after releasing the lock. Each command is bounded by the
/// client's deadline; a connection failure marks the client down. Commands are never retried.
#[derive(Clone)]
pub struct RedisHandle {
    manager: ConnectionManager,
    health: Arc<Health>,
    deadline: Duration,
}

impl RedisHandle {
    fn with_deadline(&self, deadline: Duration) -> Self {
        Self { deadline, ..self.clone() }
    }
}

async fn bounded<T>(
    health: &Health,
    deadline: Duration,
    command: impl std::future::Future<Output = RedisResult<T>>,
) -> RedisResult<T> {
    let result = match tokio::time::timeout(deadline, command).await {
        Ok(result) => result,
        Err(_) => Err(std::io::Error::from(std::io::ErrorKind::TimedOut).into()),
    };
    if let Err(err) = &result
        && is_connection_failure(err)
    {
        health.mark_down(err);
    }
    result
}

impl ConnectionLike for RedisHandle {
    fn req_packed_command<'a>(&'a mut self, cmd: &'a Cmd) -> RedisFuture<'a, Value> {
        let Self { manager, health, deadline } = self;
        Box::pin(bounded(health, *deadline, manager.send_packed_command(cmd)))
    }

    fn req_packed_commands<'a>(
        &'a mut self,
        cmd: &'a Pipeline,
        offset: usize,
        count: usize,
    ) -> RedisFuture<'a, Vec<Value>> {
        let Self { manager, health, deadline } = self;
        Box::pin(bounded(health, *deadline, manager.send_packed_commands(cmd, offset, count)))
    }

    fn get_db(&self) -> i64 {
        self.manager.get_db()
    }
}

/// PING up to `attempts` times. After a failed connection attempt the next command only
/// returns that failure and starts a new attempt (`connection_manager.rs:702-706`); a second
/// PING waits for that attempt.
async fn probe(conn: &mut RedisHandle, attempts: usize) -> bool {
    for _ in 0..attempts {
        match redis::cmd("PING").query_async::<String>(conn).await {
            Ok(_) => return true,
            Err(err) if is_connection_failure(&err) => continue,
            Err(_) => return false,
        }
    }
    false
}

async fn monitor(mut conn: RedisHandle, timings: Timings) {
    let mut failures = 0;
    loop {
        let wait =
            if conn.health.usable.load(Ordering::Acquire) { timings.probe_interval } else { timings.backoff(failures) };
        tokio::select! {
            () = tokio::time::sleep(wait) => {}
            () = conn.health.wake.notified() => {}
        }
        if probe(&mut conn, 2).await {
            conn.health.mark_up();
            failures = 0;
        } else {
            failures = failures.saturating_add(1);
            tracing::debug!(failures, "Redis probe failed");
        }
    }
}

struct Connected {
    handle: RedisHandle,
    monitor: tokio::task::JoinHandle<()>,
}

impl Drop for Connected {
    fn drop(&mut self) {
        self.monitor.abort();
    }
}

/// Optional Redis connection with automatic recovery; see the module documentation.
pub struct RedisClient {
    /// `None` when no URL is configured or it cannot be parsed: no connection, no monitor.
    connected: Option<Connected>,
}

impl RedisClient {
    /// Create a Redis client. Returns even if Redis is unavailable (graceful degradation).
    pub async fn new(config: &AppConfig) -> Self {
        Self::connect(config.redis_url.as_deref()).await
    }

    /// Connect from an explicit URL (or `None` to run without Redis). Same
    /// graceful-degradation contract as [`RedisClient::new`]; kept public so
    /// integration tests can build a client without a full `AppConfig`.
    pub async fn connect(url: Option<&str>) -> Self {
        Self::connect_with(url, Timings::DEFAULT).await
    }

    async fn connect_with(url: Option<&str>, timings: Timings) -> Self {
        let Some(url) = url else {
            tracing::info!("Redis URL not configured, running without Redis");
            return Self { connected: None };
        };
        let manager = Client::open(url).and_then(|client| {
            let config = ConnectionManagerConfig::new()
                .set_connection_timeout(Some(timings.connect_timeout))
                .set_response_timeout(Some(timings.response_timeout))
                // The monitor owns retrying; a crate retry sequence would make commands wait.
                .set_number_of_retries(0);
            ConnectionManager::new_lazy_with_config(client, config)
        });
        let manager = match manager {
            Ok(manager) => manager,
            Err(err) => {
                tracing::warn!(error = %err, "Redis client creation failed, running without Redis");
                return Self { connected: None };
            }
        };
        let health = Arc::new(Health {
            usable: AtomicBool::new(false),
            down_since: Mutex::new(Some(Instant::now())),
            wake: Notify::new(),
        });
        let handle = RedisHandle { manager, health, deadline: timings.request_deadline };
        let mut probe_conn = handle.with_deadline(timings.probe_deadline());
        // Connect before returning, as before, so the startup checks see a usable Redis.
        if probe(&mut probe_conn, 1).await {
            handle.health.usable.store(true, Ordering::Release);
            *lock(&handle.health.down_since) = None;
            tracing::info!("Redis connected");
        } else {
            tracing::warn!(
                "Redis connection failed, running without Redis until it becomes reachable (retrying in the background)"
            );
        }
        let monitor = tokio::spawn(monitor(probe_conn, timings));
        Self { connected: Some(Connected { handle, monitor }) }
    }

    /// Returns `true` while Redis is usable.
    pub fn is_connected(&self) -> bool {
        self.handle().is_some()
    }

    /// An owned handle for Redis commands, or `None` at once when Redis is not configured or
    /// currently down (the caller takes its degraded path). Release any lock on the client
    /// before running commands on it.
    pub fn handle(&self) -> Option<RedisHandle> {
        let connected = self.connected.as_ref()?;
        connected.handle.health.usable.load(Ordering::Acquire).then(|| connected.handle.clone())
    }

    /// Health check — sends a PING command and returns `true` if Redis responds.
    pub async fn check_health(&self) -> bool {
        let Some(mut conn) = self.handle() else {
            return false;
        };
        redis::cmd("PING").query_async::<String>(&mut conn).await.is_ok()
    }

    /// Probe the actual read/write path used by the OAuth/PKCE state store:
    /// `SET` a short-TTL throwaway key, then `GETDEL` it (the exact operations
    /// `cli_auth_proxy` performs). Unlike [`is_connected`](Self::is_connected)
    /// (connection usable) or [`check_health`](Self::check_health) (PING only), this
    /// verifies the connection can actually WRITE — so a reachable but read-only
    /// or ACL-restricted Redis (connects, rejects `SET`) is detected. Returns
    /// `false` if Redis is absent or any step fails.
    pub async fn probe_read_write(&self) -> bool {
        use redis::AsyncCommands;
        use std::sync::OnceLock;
        use std::sync::atomic::AtomicU64;

        let Some(mut conn) = self.handle() else {
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
        let got: redis::RedisResult<Option<String>> = redis::cmd("GETDEL").arg(&key).query_async(&mut conn).await;
        matches!(got, Ok(Some(_)))
    }
}

/// Describe and materialise the Redis state-change counter at zero.
pub fn register_metrics() {
    metrics::describe_counter!(
        "agentforge_redis_state_changes_total",
        "Redis connection state changes, by state (lost/restored)."
    );
    metrics::counter!("agentforge_redis_state_changes_total", "state" => "lost").increment(0);
    metrics::counter!("agentforge_redis_state_changes_total", "state" => "restored").increment(0);
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
            auth_sso: agentforge_core::config::SsoConfig::default(),
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
            llm_pricing: None,
            review_required_gates: None,
            compliance_export_interval_hours: 0,
            compliance_export_dir: None,
            analytics_retention_days: 0,
            run_retention_days: 0,
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

    use crate::fake_redis::{FakeRedis, Mode};
    use std::sync::Mutex as StdMutex;

    /// Short timings so the recovery tests finish quickly; same structure as production.
    const FAST: Timings = Timings {
        connect_timeout: Duration::from_millis(300),
        response_timeout: Duration::from_millis(300),
        request_deadline: Duration::from_millis(500),
        probe_interval: Duration::from_millis(100),
        backoff_min: Duration::from_millis(20),
        backoff_max: Duration::from_millis(200),
    };
    /// Scheduling headroom on a loaded CI host, added to every time bound below.
    const SLACK: Duration = Duration::from_secs(2);

    #[derive(Clone, Default)]
    struct Logs(Arc<StdMutex<Vec<u8>>>);

    impl std::io::Write for Logs {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            lock(&self.0).extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl Logs {
        /// Capture this thread's logs (the tests run on a current-thread runtime, so the
        /// monitor task logs here too).
        fn capture() -> (Self, tracing::subscriber::DefaultGuard) {
            let logs = Self::default();
            let writer = logs.clone();
            let subscriber = tracing_subscriber::fmt()
                .with_max_level(tracing::Level::INFO)
                .with_writer(move || writer.clone())
                .with_ansi(false)
                .finish();
            (logs, tracing::subscriber::set_default(subscriber))
        }

        fn count(&self, needle: &str) -> usize {
            String::from_utf8_lossy(&lock(&self.0)).matches(needle).count()
        }
    }

    const LOST: &str = "Redis connection lost";
    const RESTORED: &str = "Redis connection restored";

    /// Wait until `is_connected()` equals `want`, failing after `limit`.
    async fn wait_until(client: &RedisClient, want: bool, limit: Duration) -> Duration {
        let start = Instant::now();
        while client.is_connected() != want {
            assert!(start.elapsed() < limit, "is_connected() did not become {want} within {limit:?}");
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        start.elapsed()
    }

    /// Runs a command and asserts it returned within `limit`; returns whether it succeeded.
    async fn timed_health(client: &RedisClient, limit: Duration) -> bool {
        let start = Instant::now();
        let ok = client.check_health().await;
        assert!(start.elapsed() < limit, "check_health took {:?}, limit {limit:?}", start.elapsed());
        ok
    }

    #[tokio::test]
    async fn no_url_configured_not_connected() {
        let (logs, _guard) = Logs::capture();
        let client = RedisClient::new(&test_config(None)).await;
        assert!(!client.is_connected());
        assert!(client.handle().is_none());
        assert!(client.connected.is_none(), "no monitor task without a URL");
        assert_eq!(logs.count(LOST) + logs.count(RESTORED), 0);
    }

    #[tokio::test]
    async fn health_check_false_when_not_connected() {
        let client = RedisClient::new(&test_config(None)).await;
        assert!(!client.check_health().await);
    }

    #[tokio::test]
    async fn probe_read_write_false_when_not_connected() {
        // No connection → the CN-7 startup probe must report Redis unusable, so
        // ensure_external_state_redis_ready fails fast under REQUIRE_EXTERNAL_STATE.
        let client = RedisClient::new(&test_config(None)).await;
        assert!(!client.probe_read_write().await);
    }

    #[tokio::test]
    async fn invalid_url_degrades_gracefully() {
        // An unreachable URL should not panic — just degrade.
        let client = RedisClient::new(&test_config(Some("redis://127.0.0.1:1".to_string()))).await;
        assert!(!client.is_connected());
    }

    #[test]
    fn default_recovery_bound_matches_the_documentation() {
        assert_eq!(Timings::DEFAULT.recovery_bound(), Duration::from_millis(9500));
        for failures in 0..40 {
            let delay = Timings::DEFAULT.backoff(failures);
            assert!(delay <= Timings::DEFAULT.backoff_max, "backoff {delay:?} above the cap");
            assert!(delay >= Timings::DEFAULT.backoff_min / 2, "backoff {delay:?} below half the minimum");
        }
    }

    #[tokio::test]
    async fn recovers_after_repeated_outages_and_logs_each_change_once() {
        let (logs, _guard) = Logs::capture();
        let mut fake = FakeRedis::start().await;
        let client = RedisClient::connect_with(Some(&fake.url()), FAST).await;
        assert!(client.is_connected());
        assert!(client.probe_read_write().await);

        for outage in [Duration::ZERO, Duration::from_secs(2)] {
            fake.stop().await;
            // Commands during the outage fail within the request deadline, never hang.
            for _ in 0..5 {
                assert!(!timed_health(&client, FAST.request_deadline + SLACK).await);
            }
            assert!(!client.is_connected(), "the failed command marks the state down at once");
            assert!(client.handle().is_none());
            // The second outage outlasts many backoff steps (the crate's own retries are off).
            tokio::time::sleep(outage).await;
            fake.restart().await;
            wait_until(&client, true, FAST.recovery_bound() + SLACK).await;
            assert!(client.check_health().await);
            assert!(client.probe_read_write().await);
        }
        assert_eq!(logs.count(LOST), 2, "one lost line per outage");
        assert_eq!(logs.count(RESTORED), 2, "one restored line per recovery");
    }

    #[tokio::test]
    async fn loss_and_recovery_are_noticed_without_any_commands() {
        let (logs, _guard) = Logs::capture();
        let mut fake = FakeRedis::start().await;
        let client = RedisClient::connect_with(Some(&fake.url()), FAST).await;
        assert!(client.is_connected());
        // No request-path command at all: the monitor's interval PING finds the loss.
        fake.stop().await;
        wait_until(&client, false, FAST.probe_interval + FAST.probe_deadline() + SLACK).await;
        fake.restart().await;
        wait_until(&client, true, FAST.recovery_bound() + SLACK).await;
        assert!(client.probe_read_write().await);
        assert_eq!((logs.count(LOST), logs.count(RESTORED)), (1, 1));
    }

    #[tokio::test]
    async fn recovers_when_unreachable_at_startup() {
        let (logs, _guard) = Logs::capture();
        let addr = FakeRedis::unused_addr();
        let client = RedisClient::connect_with(Some(&FakeRedis::url_for(addr)), FAST).await;
        assert!(!client.is_connected());
        assert!(!timed_health(&client, Duration::from_millis(50)).await, "down: degrade at once");
        assert_eq!(logs.count("Redis connection failed"), 1);

        let _fake = FakeRedis::start_at(addr).await;
        wait_until(&client, true, FAST.recovery_bound() + SLACK).await;
        assert!(client.check_health().await);
        assert!(client.probe_read_write().await);
        assert_eq!((logs.count(LOST), logs.count(RESTORED)), (0, 1));
    }

    #[tokio::test]
    async fn unanswered_command_returns_within_the_deadline_and_marks_down() {
        // A response timeout longer than the deadline, so only the wrapper's deadline can end it.
        let timings = Timings { response_timeout: Duration::from_secs(30), ..FAST };
        let mut fake = FakeRedis::start().await;
        let client = RedisClient::connect_with(Some(&fake.url()), timings).await;
        assert!(client.is_connected());

        fake.set_mode(Mode::Blackhole);
        assert!(!timed_health(&client, timings.request_deadline + SLACK).await);
        assert!(!client.is_connected(), "a deadline marks the state down at once");

        // The server comes back as a fresh process: connections dropped, answering again.
        fake.set_mode(Mode::Normal);
        fake.restart().await;
        wait_until(&client, true, timings.recovery_bound() + SLACK).await;
        assert!(client.probe_read_write().await);
    }

    #[tokio::test]
    async fn recovers_after_connections_drop_during_reconnection() {
        let mut fake = FakeRedis::start().await;
        let client = RedisClient::connect_with(Some(&fake.url()), FAST).await;
        fake.set_mode(Mode::DropOnAccept);
        fake.restart().await;
        wait_until(&client, false, FAST.probe_interval + FAST.probe_deadline() + SLACK).await;
        // Listening, but every reconnection attempt is closed during the handshake.
        tokio::time::sleep(FAST.recovery_bound()).await;
        assert!(!client.is_connected());
        fake.set_mode(Mode::Normal);
        wait_until(&client, true, FAST.recovery_bound() + SLACK).await;
        assert!(client.probe_read_write().await);
    }

    #[tokio::test]
    async fn late_reply_after_the_deadline_is_a_failure_and_never_replayed() {
        use redis::AsyncCommands;
        let timings = Timings { response_timeout: Duration::from_secs(30), ..FAST };
        let fake = FakeRedis::start().await;
        let client = RedisClient::connect_with(Some(&fake.url()), timings).await;
        let mut conn = client.handle().expect("usable");
        let () = conn.set_ex("state", "v", 60).await.expect("set");

        let delay = timings.request_deadline * 3;
        fake.set_mode(Mode::Delay(delay));
        let taken: RedisResult<Option<String>> = redis::cmd("GETDEL").arg("state").query_async(&mut conn).await;
        assert!(taken.is_err(), "the caller sees the timed-out GETDEL as a failure");
        assert!(!client.is_connected());

        // The late reply arrives; the server ran the command once and nothing re-sent it.
        tokio::time::sleep(delay + Duration::from_millis(200)).await;
        fake.set_mode(Mode::Normal);
        wait_until(&client, true, timings.recovery_bound() + SLACK).await;
        assert_eq!(fake.received("GETDEL"), 1, "no replay");
        let mut conn = client.handle().expect("usable");
        let left: Option<String> = conn.get("state").await.expect("get");
        assert_eq!(left, None, "the timed-out GETDEL did run on the server");
    }

    #[tokio::test]
    async fn read_only_redis_is_connected_but_not_writable() {
        let fake = FakeRedis::start().await;
        fake.set_mode(Mode::ReadOnly);
        let client = RedisClient::connect_with(Some(&fake.url()), FAST).await;
        assert!(client.check_health().await);
        assert!(!client.probe_read_write().await, "a rejected SET is reported unusable");
        assert!(client.is_connected(), "a server error reply is not a connection failure");
    }

    #[tokio::test]
    async fn dropping_the_client_stops_the_monitor() {
        let fake = FakeRedis::start().await;
        let client = RedisClient::connect_with(Some(&fake.url()), FAST).await;
        let health = Arc::downgrade(&client.connected.as_ref().expect("configured").handle.health);
        drop(client);
        let start = Instant::now();
        while health.upgrade().is_some() {
            assert!(start.elapsed() < SLACK, "monitor task still holds the client state");
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }
}

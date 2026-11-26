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
//!   crate's own reconnection retries switched off. While a manager reconnects, commands
//!   issued after a failure wait for that reconnection outside the response timeout
//!   (`redis-1.8.2/src/aio/connection_manager.rs:380-382`, `:702-706`); it keeps a connection
//!   that only times out (`:405-413`) and keeps a failed setup such as a rejected `AUTH`
//!   (`:417-426`). So request paths never use a manager while the state is down
//!   ([`RedisClient::handle`] returns `None` at once), and recovery never reuses one: every
//!   probe while down builds a fresh manager, and only one that answered replaces the old one.
//! - Every request-path command runs under one overall deadline
//!   (`Timings::DEFAULT.request_deadline`, 1 s). A deadline or connection error marks the
//!   state down immediately (the next caller degrades without waiting) and wakes the monitor.
//!   Failures reported through a replaced manager are ignored.
//! - One background monitor task per client owns recovery. While usable it PINGs every 5 s,
//!   so a loss is noticed even when nothing uses Redis; while down it probes a fresh
//!   connection with capped exponential backoff (250 ms doubling to 5 s, with jitter) until
//!   Redis answers, forever. It ends when the client is dropped. Being unreachable at startup
//!   is the same case as losing the connection later.
//! - Redis is used again at most `Timings::recovery_bound` (8 s) after it is reachable, also
//!   when the old socket is never closed (a silent firewall, a host gone without a FIN).
//! - Commands are never replayed. A command that timed out may still have run on the server
//!   (for example a `GETDEL` that consumed OAuth state), so callers must keep treating a
//!   timeout as a failure of that operation.
//! - The usable flag, the outage start and the failure generation change together under one
//!   short lock that is never held across an await and never around anything that can block:
//!   under it a transition is only recorded as an event in a bounded channel (non-blocking
//!   send; a full channel counts the event as dropped). A reporter task writes the lines
//!   afterwards, in transition order: one line per change (`warn` when lost, `info` with the
//!   outage length when restored, both with `dropped_state_changes`). The thread that made a
//!   change counts it in `agentforge_redis_state_changes_total{state}` right after releasing
//!   the lock, so the metric stays exact even when log lines are dropped.
//!
//! The in-process fake Redis used by the tests (`crate::fake_redis`) is compiled only for this
//! crate's tests or when the `test-support` feature is enabled explicitly; nothing in the
//! workspace enables that feature outside tests (`agentforge-api` turns it on only in its
//! `[dev-dependencies]`).

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use agentforge_core::AppConfig;
use redis::aio::{ConnectionLike, ConnectionManager, ConnectionManagerConfig};
use redis::{Client, Cmd, Pipeline, RedisError, RedisFuture, RedisResult, Value};
use tokio::sync::{Notify, mpsc};
use tracing::instrument::WithSubscriber;

/// State-change events waiting for the reporter; transitions are rare, so this never fills
/// in practice.
const EVENT_BUFFER: usize = 64;

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

    /// One probe: a connection attempt (fresh manager) or an existing connection, then a reply.
    fn probe_deadline(&self) -> Duration {
        self.connect_timeout + self.response_timeout
    }

    /// Longest time from "Redis is reachable" to "the state is usable". While down: a probe
    /// that started just before may still fail, the monitor waits at most one backoff step,
    /// and the next probe on a fresh connection succeeds. While usable with a dead or silent
    /// socket: at most one interval to the next PING, which fails within a probe deadline,
    /// then at once a probe on a fresh connection.
    #[cfg(test)]
    fn recovery_bound(&self) -> Duration {
        self.probe_deadline() * 2 + self.probe_interval.max(self.backoff_max)
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

fn timed_out() -> RedisError {
    std::io::Error::from(std::io::ErrorKind::TimedOut).into()
}

async fn within<T>(deadline: Duration, command: impl Future<Output = RedisResult<T>>) -> RedisResult<T> {
    tokio::time::timeout(deadline, command).await.unwrap_or_else(|_| Err(timed_out()))
}

/// Everything that changes together; only touched under [`Shared::state`].
struct State {
    usable: bool,
    /// The manager handles are cut from; replaced on recovery.
    manager: ConnectionManager,
    /// Bumped on every replacement, so failures seen through an older manager are ignored.
    epoch: u64,
    /// Bumped on every loss; a probe that started before the latest loss cannot mark usable.
    generation: u64,
    /// Start of the current outage; `None` exactly while usable.
    down_since: Option<Instant>,
}

/// A change of state, decided under the state lock and reported after it.
#[derive(Debug)]
enum Event {
    Lost { error: String },
    Restored { down_for: Duration },
}

struct Shared {
    client: Client,
    config: ConnectionManagerConfig,
    state: Mutex<State>,
    /// Wakes the monitor when a request-path command finds Redis gone.
    wake: Notify,
    events: mpsc::Sender<Event>,
    /// Events the full channel could not take; reported with the next line.
    dropped: Arc<AtomicU64>,
}

impl Shared {
    /// Called under the state lock: never blocks.
    fn record(&self, event: Event) {
        if self.events.try_send(event).is_err() {
            self.dropped.fetch_add(1, Ordering::Relaxed);
        }
    }

    fn fresh_manager(&self) -> RedisResult<ConnectionManager> {
        ConnectionManager::new_lazy_with_config(self.client.clone(), self.config.clone())
    }

    fn handle(self: &Arc<Self>, deadline: Duration) -> Option<RedisHandle> {
        let state = lock(&self.state);
        state.usable.then(|| RedisHandle {
            manager: state.manager.clone(),
            epoch: state.epoch,
            shared: self.clone(),
            deadline,
        })
    }

    fn generation(&self) -> u64 {
        lock(&self.state).generation
    }

    /// A command through `epoch`'s manager hit a connection failure.
    fn report_failure(&self, epoch: u64, err: &RedisError) {
        let mut state = lock(&self.state);
        if state.epoch != epoch || !state.usable {
            return;
        }
        state.usable = false;
        state.generation += 1;
        state.down_since = Some(Instant::now());
        self.record(Event::Lost { error: err.to_string() });
        drop(state);
        // Counted here, after the lock, so the metric never depends on the log queue.
        metrics::counter!("agentforge_redis_state_changes_total", "state" => "lost").increment(1);
        self.wake.notify_one();
    }

    /// A probe that started at `generation` got an answer through `manager`: install it and
    /// mark usable, unless Redis was lost again since the probe started.
    fn report_success(&self, generation: u64, manager: ConnectionManager) -> bool {
        let mut state = lock(&self.state);
        if state.generation != generation {
            return false;
        }
        state.manager = manager;
        state.epoch += 1;
        if state.usable {
            return true;
        }
        state.usable = true;
        let down_for = state.down_since.take().map(|since| since.elapsed()).unwrap_or_default();
        self.record(Event::Restored { down_for });
        drop(state);
        metrics::counter!("agentforge_redis_state_changes_total", "state" => "restored").increment(1);
        true
    }
}

/// An owned, cheap Redis connection handle from [`RedisClient::handle`]. Take it under a short
/// lock on the client and run commands after releasing the lock. Each command is bounded by the
/// client's deadline; a connection failure marks the client down. Commands are never retried.
#[derive(Clone)]
pub struct RedisHandle {
    manager: ConnectionManager,
    epoch: u64,
    shared: Arc<Shared>,
    deadline: Duration,
}

async fn bounded<T>(
    shared: &Shared,
    epoch: u64,
    deadline: Duration,
    command: impl Future<Output = RedisResult<T>>,
) -> RedisResult<T> {
    let result = within(deadline, command).await;
    if let Err(err) = &result
        && is_connection_failure(err)
    {
        shared.report_failure(epoch, err);
    }
    result
}

impl ConnectionLike for RedisHandle {
    fn req_packed_command<'a>(&'a mut self, cmd: &'a Cmd) -> RedisFuture<'a, Value> {
        let Self { manager, epoch, shared, deadline } = self;
        Box::pin(bounded(shared, *epoch, *deadline, manager.send_packed_command(cmd)))
    }

    fn req_packed_commands<'a>(
        &'a mut self,
        cmd: &'a Pipeline,
        offset: usize,
        count: usize,
    ) -> RedisFuture<'a, Vec<Value>> {
        let Self { manager, epoch, shared, deadline } = self;
        Box::pin(bounded(shared, *epoch, *deadline, manager.send_packed_commands(cmd, offset, count)))
    }

    fn get_db(&self) -> i64 {
        self.manager.get_db()
    }
}

impl RedisHandle {
    /// Health check — sends a PING command and returns `true` if Redis responds.
    pub async fn check_health(mut self) -> bool {
        redis::cmd("PING").query_async::<String>(&mut self).await.is_ok()
    }

    /// Probe the actual read/write path used by the OAuth/PKCE state store:
    /// `SET` a short-TTL throwaway key, then `GETDEL` it (the exact operations
    /// `cli_auth_proxy` performs). Unlike [`check_health`](Self::check_health) (PING only),
    /// this verifies the connection can actually WRITE — so a reachable but read-only
    /// or ACL-restricted Redis (connects, rejects `SET`) is detected.
    pub async fn probe_read_write(mut self) -> bool {
        use redis::AsyncCommands;
        use std::sync::OnceLock;
        use std::sync::atomic::{AtomicU64, Ordering};

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
        if self.set_ex::<_, _, ()>(&key, "1", 10).await.is_err() {
            return false;
        }
        let got: redis::RedisResult<Option<String>> = redis::cmd("GETDEL").arg(&key).query_async(&mut self).await;
        matches!(got, Ok(Some(_)))
    }
}

/// PING a manager that is not installed (its failures report nothing).
async fn ping(manager: &mut ConnectionManager, deadline: Duration) -> bool {
    within(deadline, redis::cmd("PING").query_async::<String>(manager)).await.is_ok()
}

/// One probe while down: a fresh manager, so neither a silent old socket nor a cached failed
/// setup (a rejected `AUTH`) is reused. Installed only if it answered.
async fn probe_fresh(shared: &Shared, deadline: Duration) -> bool {
    let generation = shared.generation();
    let Ok(mut candidate) = shared.fresh_manager() else {
        return false;
    };
    ping(&mut candidate, deadline).await && shared.report_success(generation, candidate)
}

async fn monitor(shared: Arc<Shared>, timings: Timings) {
    let mut failures = 0;
    loop {
        let usable = lock(&shared.state).usable;
        let wait = if usable { timings.probe_interval } else { timings.backoff(failures) };
        tokio::select! {
            () = tokio::time::sleep(wait) => {}
            () = shared.wake.notified() => {}
        }
        // While usable, PING the installed manager; a failure reports the loss and wakes us.
        if let Some(conn) = shared.handle(timings.probe_deadline()) {
            conn.check_health().await;
            continue;
        }
        if probe_fresh(&shared, timings.probe_deadline()).await {
            failures = 0;
        } else {
            failures = failures.saturating_add(1);
            tracing::debug!(failures, "Redis probe failed");
        }
    }
}

/// Writes state-change lines in transition order, outside the state lock. Ends
/// when the client's shared state is dropped.
async fn report(mut events: mpsc::Receiver<Event>, dropped: Arc<AtomicU64>) {
    while let Some(event) = events.recv().await {
        let dropped = dropped.swap(0, Ordering::Relaxed);
        match event {
            Event::Lost { error } => {
                tracing::warn!(
                    error,
                    dropped_state_changes = dropped,
                    "Redis connection lost; Redis-backed features are degraded until it is restored"
                );
            }
            Event::Restored { down_for } => {
                tracing::info!(
                    down_for_ms = down_for.as_millis() as u64,
                    dropped_state_changes = dropped,
                    "Redis connection restored"
                );
            }
        }
    }
}

struct Connected {
    shared: Arc<Shared>,
    request_deadline: Duration,
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
        let config = ConnectionManagerConfig::new()
            .set_connection_timeout(Some(timings.connect_timeout))
            .set_response_timeout(Some(timings.response_timeout))
            // The monitor owns retrying; a crate retry sequence would make commands wait.
            .set_number_of_retries(0);
        let built = Client::open(url).and_then(|client| {
            let manager = ConnectionManager::new_lazy_with_config(client.clone(), config.clone())?;
            Ok((client, manager))
        });
        let (client, manager) = match built {
            Ok(built) => built,
            Err(err) => {
                tracing::warn!(error = %err, "Redis client creation failed, running without Redis");
                return Self { connected: None };
            }
        };
        let (events, receiver) = mpsc::channel(EVENT_BUFFER);
        let dropped = Arc::new(AtomicU64::new(0));
        // Logs through the subscriber current here (the global one in the server).
        tokio::spawn(report(receiver, dropped.clone()).with_current_subscriber());
        let shared = Arc::new(Shared {
            client,
            config,
            events,
            dropped,
            state: Mutex::new(State {
                usable: false,
                manager: manager.clone(),
                epoch: 0,
                generation: 0,
                down_since: Some(Instant::now()),
            }),
            wake: Notify::new(),
        });
        // Connect before returning, as before, so the startup checks see a usable Redis.
        let mut first = manager;
        if ping(&mut first, timings.probe_deadline()).await {
            {
                let mut state = lock(&shared.state);
                state.usable = true;
                state.down_since = None;
            }
            tracing::info!("Redis connected");
        } else {
            tracing::warn!(
                "Redis connection failed, running without Redis until it becomes reachable (retrying in the background)"
            );
        }
        let monitor = tokio::spawn(monitor(shared.clone(), timings));
        Self { connected: Some(Connected { shared, request_deadline: timings.request_deadline, monitor }) }
    }

    /// Returns `true` while Redis is usable.
    pub fn is_connected(&self) -> bool {
        self.connected.as_ref().is_some_and(|connected| lock(&connected.shared.state).usable)
    }

    /// An owned handle for Redis commands, or `None` at once when Redis is not configured or
    /// currently down (the caller takes its degraded path). Release any lock on the client
    /// before running commands on it.
    pub fn handle(&self) -> Option<RedisHandle> {
        let connected = self.connected.as_ref()?;
        connected.shared.handle(connected.request_deadline)
    }

    /// [`RedisHandle::check_health`], or `false` without a usable Redis.
    pub async fn check_health(&self) -> bool {
        match self.handle() {
            Some(conn) => conn.check_health().await,
            None => false,
        }
    }

    /// [`RedisHandle::probe_read_write`], or `false` without a usable Redis.
    pub async fn probe_read_write(&self) -> bool {
        match self.handle() {
            Some(conn) => conn.probe_read_write().await,
            None => false,
        }
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
                .with_max_level(tracing::Level::DEBUG)
                .with_writer(move || writer.clone())
                .with_ansi(false)
                .finish();
            (logs, tracing::subscriber::set_default(subscriber))
        }

        fn text(&self) -> String {
            String::from_utf8_lossy(&lock(&self.0)).into_owned()
        }

        fn count(&self, needle: &str) -> usize {
            self.text().matches(needle).count()
        }
    }

    const LOST: &str = "Redis connection lost";
    const RESTORED: &str = "Redis connection restored";

    /// Wait until the reporter task has written exactly these state-change lines.
    async fn expect_lines(logs: &Logs, lost: usize, restored: usize) {
        let start = Instant::now();
        while (logs.count(LOST), logs.count(RESTORED)) != (lost, restored) {
            assert!(start.elapsed() < SLACK, "expected {lost} lost / {restored} restored lines:\n{}", logs.text());
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

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

    /// The number quoted in the module docs, `docs/guides/configuration.md`,
    /// `docs/runbooks/redis-recovery.md` and the CHANGELOG.
    const DOCUMENTED_RECOVERY_BOUND: Duration = Duration::from_secs(8);

    #[test]
    fn default_recovery_bound_matches_the_documentation() {
        let t = Timings::DEFAULT;
        // A probe that just failed, one wait, one successful probe; each probe is one
        // connection attempt plus one reply.
        let derived = (t.connect_timeout + t.response_timeout) * 2 + t.probe_interval.max(t.backoff_max);
        assert_eq!(t.recovery_bound(), derived);
        assert_eq!(DOCUMENTED_RECOVERY_BOUND, derived, "update the documented number");
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
        expect_lines(&logs, 2, 2).await;
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
        expect_lines(&logs, 1, 1).await;
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
        expect_lines(&logs, 0, 1).await;
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
        let shared = Arc::downgrade(&client.connected.as_ref().expect("configured").shared);
        drop(client);
        let start = Instant::now();
        while shared.upgrade().is_some() {
            assert!(start.elapsed() < SLACK, "monitor task still holds the client state");
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }
    #[tokio::test]
    async fn recovers_when_the_old_socket_stays_open_and_silent() {
        let fake = FakeRedis::start().await;
        let client = RedisClient::connect_with(Some(&fake.url()), FAST).await;
        assert!(client.is_connected());
        // The old connection is never closed and never answers again; new ones are answered.
        fake.silence_open_connections();
        assert!(!timed_health(&client, FAST.request_deadline + SLACK).await);
        wait_until(&client, true, FAST.recovery_bound() + SLACK).await;
        assert!(client.check_health().await);
        assert!(client.probe_read_write().await);
    }

    #[tokio::test]
    async fn recovers_after_auth_is_denied_and_allowed_again() {
        let (logs, _guard) = Logs::capture();
        let mut fake = FakeRedis::start().await;
        let password = uuid::Uuid::new_v4().simple().to_string();
        let client = RedisClient::connect_with(Some(&fake.url_with_password(&password)), FAST).await;
        assert!(client.is_connected());

        // The Redis user is disabled and the connection drops: every new AUTH is rejected.
        fake.set_mode(Mode::DenyAuth);
        fake.restart().await;
        assert!(!timed_health(&client, FAST.request_deadline + SLACK).await);
        tokio::time::sleep(FAST.recovery_bound()).await;
        assert!(!client.is_connected(), "AUTH is still denied");

        // Access restored: the same client recovers instead of keeping the cached failure.
        fake.set_mode(Mode::Normal);
        wait_until(&client, true, FAST.recovery_bound() + SLACK).await;
        assert!(client.probe_read_write().await);
        assert!(fake.received("AUTH") > 2, "the client kept trying to authenticate");
        assert!(!logs.text().contains(&password), "no credential in any log line");
    }

    /// Asserts the state invariant: the outage start is set exactly while down.
    fn assert_consistent(shared: &Shared) {
        let state = lock(&shared.state);
        assert_eq!(state.usable, state.down_since.is_none(), "flag and outage start disagree");
    }

    #[tokio::test]
    async fn a_probe_that_started_before_a_loss_cannot_mark_usable() {
        let (logs, _guard) = Logs::capture();
        let fake = FakeRedis::start().await;
        let client = RedisClient::connect_with(Some(&fake.url()), FAST).await;
        let shared = &client.connected.as_ref().expect("configured").shared;
        // Driven synchronously, so the monitor task cannot run in between.
        let epoch = lock(&shared.state).epoch;
        let probe_started = shared.generation();
        shared.report_failure(epoch, &timed_out());
        assert_consistent(shared);
        let stale = shared.fresh_manager().expect("manager");
        assert!(!shared.report_success(probe_started, stale), "older probe overwrote a newer failure");
        assert!(!client.is_connected());
        assert_consistent(shared);

        let fresh = shared.fresh_manager().expect("manager");
        assert!(shared.report_success(shared.generation(), fresh));
        assert!(client.is_connected());
        assert_consistent(shared);
        expect_lines(&logs, 1, 1).await;
        let text = logs.text();
        let (lost, restored) = (text.find(LOST).expect("lost line"), text.find(RESTORED).expect("restored line"));
        assert!(lost < restored, "restored logged before lost");
    }

    #[tokio::test]
    async fn restore_then_loss_keep_flag_outage_and_log_in_step() {
        let (logs, _guard) = Logs::capture();
        let addr = FakeRedis::unused_addr();
        let client = RedisClient::connect_with(Some(&FakeRedis::url_for(addr)), FAST).await;
        let shared = &client.connected.as_ref().expect("configured").shared;
        let old_epoch = lock(&shared.state).epoch;
        assert_consistent(shared);

        // Restored first, then a loss: both complete in that order, nothing left behind.
        assert!(shared.report_success(shared.generation(), shared.fresh_manager().expect("manager")));
        assert_consistent(shared);
        // A failure seen through the replaced manager does not touch the new one.
        shared.report_failure(old_epoch, &timed_out());
        assert!(client.is_connected());
        let epoch = lock(&shared.state).epoch;
        shared.report_failure(epoch, &timed_out());
        assert!(!client.is_connected());
        assert_consistent(shared);
        assert!(lock(&shared.state).down_since.is_some());
        expect_lines(&logs, 1, 1).await;
        let text = logs.text();
        assert!(text.find(RESTORED).expect("restored") < text.find(LOST).expect("lost"));
    }

    /// A log writer that blocks every write while its gate is closed (a backed-up stdout pipe).
    #[derive(Clone)]
    struct GatedLogs {
        logs: Logs,
        gate: Arc<(StdMutex<bool>, std::sync::Condvar)>,
        waiting: Arc<std::sync::atomic::AtomicBool>,
    }

    impl std::io::Write for GatedLogs {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            let (open, changed) = &*self.gate;
            let mut open = lock(open);
            while !*open {
                self.waiting.store(true, Ordering::SeqCst);
                open = changed.wait(open).unwrap_or_else(PoisonError::into_inner);
            }
            drop(open);
            self.logs.write(bytes)
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn a_blocked_log_writer_does_not_block_redis_callers() {
        let writer = GatedLogs {
            logs: Logs::default(),
            gate: Arc::new((StdMutex::new(true), std::sync::Condvar::new())),
            waiting: Arc::default(),
        };
        let (logs, gate, waiting) = (writer.logs.clone(), writer.gate.clone(), writer.waiting.clone());
        let set_gate = |is_open: bool| {
            *lock(&gate.0) = is_open;
            gate.1.notify_all();
        };
        let subscriber = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::INFO)
            .with_writer(move || writer.clone())
            .with_ansi(false)
            .finish();
        let _guard = tracing::subscriber::set_default(subscriber);
        // Callers' tasks log through this subscriber too (the global one in the server).
        // The monitor only probes when woken, and its probes fail: only this test changes state.
        let hour = Duration::from_secs(3600);
        let quiet = Timings { probe_interval: hour, backoff_min: hour, backoff_max: hour, ..FAST };
        let mut fake = FakeRedis::start().await;
        let client = Arc::new(RedisClient::connect_with(Some(&fake.url()), quiet).await);
        assert!(client.is_connected());
        fake.stop().await;
        set_gate(false);

        // First transition: the reporter picks it up and blocks in the writer.
        let first = {
            let client = client.clone();
            tokio::spawn(
                async move {
                    let shared = &client.connected.as_ref().expect("configured").shared;
                    let epoch = lock(&shared.state).epoch;
                    shared.report_failure(epoch, &timed_out());
                }
                .with_current_subscriber(),
            )
        };
        let start = Instant::now();
        while !waiting.load(Ordering::SeqCst) && start.elapsed() < SLACK {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        // While the writer is blocked, callers still get handles and report transitions. The
        // callers log through the same subscriber, as every thread does in the server.
        let others = {
            let client = client.clone();
            tokio::spawn(
                async move {
                    let shared = &client.connected.as_ref().expect("configured").shared;
                    let down = client.handle().is_none();
                    let restored = shared.report_success(shared.generation(), shared.fresh_manager().expect("manager"));
                    let usable = client.handle().is_some();
                    let epoch = lock(&shared.state).epoch;
                    shared.report_failure(epoch, &timed_out());
                    down && restored && usable && client.handle().is_none()
                }
                .with_current_subscriber(),
            )
        };
        let first = tokio::time::timeout(Duration::from_secs(2), first).await;
        let others = tokio::time::timeout(Duration::from_secs(2), others).await;
        let was_waiting = waiting.load(Ordering::SeqCst);
        // Release before asserting, so a failure cannot leave a worker blocked forever.
        set_gate(true);
        assert!(was_waiting, "the log writer was never reached");
        assert!(matches!(first, Ok(Ok(()))), "report_failure blocked behind the log writer: {first:?}");
        assert!(matches!(others, Ok(Ok(true))), "a caller blocked behind the log writer: {others:?}");

        expect_lines(&logs, 2, 1).await;
        let text = logs.text();
        let (lost, restored, lost_again) =
            (text.find(LOST).expect("lost"), text.find(RESTORED).expect("restored"), text.rfind(LOST).expect("lost"));
        assert!(lost < restored && restored < lost_again, "lines out of transition order:\n{text}");
    }
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn state_change_metric_counts_every_transition_when_log_lines_are_dropped() {
        use metrics_util::debugging::{DebugValue, DebuggingRecorder};
        let writer = GatedLogs {
            logs: Logs::default(),
            gate: Arc::new((StdMutex::new(true), std::sync::Condvar::new())),
            waiting: Arc::default(),
        };
        let (logs, gate, waiting) = (writer.logs.clone(), writer.gate.clone(), writer.waiting.clone());
        let set_gate = |is_open: bool| {
            *lock(&gate.0) = is_open;
            gate.1.notify_all();
        };
        let subscriber = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::INFO)
            .with_writer(move || writer.clone())
            .with_ansi(false)
            .finish();
        let _guard = tracing::subscriber::set_default(subscriber);
        let hour = Duration::from_secs(3600);
        let quiet = Timings { probe_interval: hour, backoff_min: hour, backoff_max: hour, ..FAST };
        let mut fake = FakeRedis::start().await;
        let client = RedisClient::connect_with(Some(&fake.url()), quiet).await;
        let shared = &client.connected.as_ref().expect("configured").shared;
        fake.stop().await;
        set_gate(false);

        // Far more transitions than the event channel holds while the log writer is stuck.
        const CYCLES: usize = EVENT_BUFFER * 3;
        let recorder = DebuggingRecorder::new();
        let snapshotter = recorder.snapshotter();
        let writer_was_blocked = metrics::with_local_recorder(&recorder, || {
            let epoch = lock(&shared.state).epoch;
            shared.report_failure(epoch, &timed_out());
            // The reporter takes that event and blocks in the writer (bounded wait).
            let start = Instant::now();
            while !waiting.load(Ordering::SeqCst) && start.elapsed() < SLACK {
                std::thread::sleep(Duration::from_millis(5));
            }
            for _ in 0..CYCLES {
                assert!(shared.report_success(shared.generation(), shared.fresh_manager().expect("manager")));
                let epoch = lock(&shared.state).epoch;
                shared.report_failure(epoch, &timed_out());
            }
            waiting.load(Ordering::SeqCst)
        });
        let lines_while_blocked = logs.count(LOST) + logs.count(RESTORED);
        // Release before asserting, so a failure cannot leave the reporter blocked forever.
        set_gate(true);

        // One snapshot: taking it drains the recorder's counters.
        let snapshot = snapshotter.snapshot().into_vec();
        let counted = |state: &str| {
            snapshot
                .iter()
                .find(|(key, ..)| {
                    key.key().name() == "agentforge_redis_state_changes_total"
                        && key.key().labels().any(|label| label.key() == "state" && label.value() == state)
                })
                .map(|(.., value)| value)
        };
        assert!(writer_was_blocked, "the log writer was never reached");
        assert_eq!(lines_while_blocked, 0, "no line can be written while the writer is blocked");
        assert_eq!(counted("lost"), Some(&DebugValue::Counter(CYCLES as u64 + 1)), "every loss is counted");
        assert_eq!(counted("restored"), Some(&DebugValue::Counter(CYCLES as u64)), "every restore is counted");

        // One event was in the writer and the channel held EVENT_BUFFER; the rest were dropped,
        // and the next line written says how many.
        let transitions = 2 * CYCLES + 1;
        let written = 1 + EVENT_BUFFER;
        let start = Instant::now();
        while logs.count(LOST) + logs.count(RESTORED) < written {
            assert!(start.elapsed() < SLACK, "queued lines were not written after release");
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        let dropped = transitions - written;
        assert!(
            logs.text().contains(&format!("dropped_state_changes={dropped}")),
            "the next line must report {dropped} dropped state changes:\n{}",
            logs.text()
        );
    }
}

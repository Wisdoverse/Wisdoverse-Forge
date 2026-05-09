//! Worker bridge: consume orchestration assignments from NATS, invoke the
//! wrapped CLI as a subprocess, and publish the result back over JetStream so
//! the backend turns it into a DB complete/fail even across a short restart.
//!
//! Assignments arrive on `orchestration.assigned.<agent_id>`. Results leave on
//! `orchestration.result.<agent_id>` as an HMAC-signed envelope so the backend
//! consumer can trust the payload. Each assignment is handled in its own
//! spawned task so a long-running CLI does not block subsequent dispatches;
//! callers that want strict serialization should assign one-at-a-time from the
//! dispatcher side instead.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use agentforge_core::CliToolKind;
use agentforge_core::orchestration_protocol::{
    ORCHESTRATION_ASSIGNMENTS_STREAM, RESULT_SUBJECT_PREFIX, SignedEnvelope, TaskAssignment, TaskOutcome, TaskResult,
    assign_subject, assignment_consumer_name,
};
use anyhow::{Context, Result};
use async_nats::Client;
use async_nats::jetstream::consumer::{self, PullConsumer, pull};
use async_nats::jetstream::{self, AckKind};
use chrono::Utc;
use futures::StreamExt;
use tokio::fs::{self, OpenOptions};
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tokio::sync::Mutex;

const ASSIGNMENT_FETCH_TIMEOUT_MS: u64 = 500;
const ASSIGNMENT_FETCH_BATCH_SIZE: usize = 1;
const ASSIGNMENT_ACK_WAIT_SECS: u64 = 300;
const ASSIGNMENT_MAX_DELIVER: i64 = 20;
const ASSIGNMENT_REPLAY_INTERVAL_SECS: u64 = 5;
const COMPLETED_TOMBSTONE_MAX_AGE_SECS: u64 = 24 * 60 * 60;
const DEFAULT_CLI_TIMEOUT_SECS: u64 = 15 * 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DispatchAction {
    Ack,
    Nak,
    Term,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AssignmentInboxState {
    Accepted,
    Pending,
    Completed,
}

#[derive(Clone, Debug)]
struct AssignmentInbox {
    pending_dir: PathBuf,
    results_dir: PathBuf,
    completed_dir: PathBuf,
}

impl AssignmentInbox {
    fn new(wal_path: Option<&str>) -> Self {
        let root = PathBuf::from(wal_path.unwrap_or("/tmp/agentforge-wal")).join("orchestration-inbox");
        Self {
            pending_dir: root.join("pending"),
            results_dir: root.join("results"),
            completed_dir: root.join("completed"),
        }
    }

    async fn accept(&self, assignment: &TaskAssignment) -> Result<AssignmentInboxState> {
        let delivery_id = assignment.delivery_id.context("assignment missing delivery_id")?;
        self.ensure_dirs().await?;

        let completed = self.completed_path(delivery_id);
        if path_exists(&completed).await? {
            return Ok(AssignmentInboxState::Completed);
        }

        let pending = self.pending_path(delivery_id);
        if path_exists(&pending).await? {
            return Ok(AssignmentInboxState::Pending);
        }

        let temp = self.temp_path(delivery_id);
        if path_exists(&temp).await? {
            fs::remove_file(&temp)
                .await
                .with_context(|| format!("remove stale assignment inbox temp file {}", temp.display()))?;
        }

        let bytes = serde_json::to_vec(assignment).context("serialize assignment inbox payload")?;
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)
            .await
            .with_context(|| format!("open assignment inbox temp file {}", temp.display()))?;
        file.write_all(&bytes).await.context("write assignment inbox payload")?;
        file.flush().await.context("flush assignment inbox payload")?;
        file.sync_data().await.context("sync assignment inbox payload")?;
        drop(file);

        match fs::rename(&temp, &pending).await {
            Ok(()) => {
                sync_dir(self.pending_dir.clone()).await?;
                Ok(AssignmentInboxState::Accepted)
            }
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {
                let _ = fs::remove_file(&temp).await;
                Ok(AssignmentInboxState::Pending)
            }
            Err(err) => {
                Err(err).with_context(|| format!("rename assignment inbox {} -> {}", temp.display(), pending.display()))
            }
        }
    }

    async fn pending_assignments(&self) -> Result<Vec<TaskAssignment>> {
        if !path_exists(&self.pending_dir).await? {
            return Ok(Vec::new());
        }

        let mut paths = Vec::new();
        let mut dir = fs::read_dir(&self.pending_dir).await.context("read assignment inbox pending dir")?;
        while let Some(entry) = dir.next_entry().await.context("iterate assignment inbox pending dir")? {
            if entry.path().extension().is_some_and(|ext| ext == "json") {
                paths.push(entry.path());
            }
        }
        paths.sort();

        let mut out = Vec::with_capacity(paths.len());
        for path in paths {
            let bytes = fs::read(&path)
                .await
                .with_context(|| format!("read pending orchestration assignment {}", path.display()))?;
            let assignment = serde_json::from_slice(&bytes)
                .with_context(|| format!("decode pending orchestration assignment {}", path.display()))?;
            out.push(assignment);
        }
        Ok(out)
    }

    async fn store_result(&self, result: &TaskResult) -> Result<()> {
        let delivery_id = result.delivery_id.context("result missing delivery_id")?;
        self.ensure_dirs().await?;

        if path_exists(&self.completed_path(delivery_id)).await? || path_exists(&self.result_path(delivery_id)).await? {
            return Ok(());
        }

        let temp = self.result_temp_path(delivery_id);
        if path_exists(&temp).await? {
            fs::remove_file(&temp)
                .await
                .with_context(|| format!("remove stale orchestration result temp file {}", temp.display()))?;
        }

        let bytes = serde_json::to_vec(result).context("serialize orchestration result outbox payload")?;
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)
            .await
            .with_context(|| format!("open orchestration result temp file {}", temp.display()))?;
        file.write_all(&bytes).await.context("write orchestration result outbox payload")?;
        file.flush().await.context("flush orchestration result outbox payload")?;
        file.sync_data().await.context("sync orchestration result outbox payload")?;
        drop(file);

        let result_path = self.result_path(delivery_id);
        match fs::rename(&temp, &result_path).await {
            Ok(()) => {
                sync_dir(self.results_dir.clone()).await?;
                Ok(())
            }
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {
                let _ = fs::remove_file(&temp).await;
                Ok(())
            }
            Err(err) => Err(err).with_context(|| {
                format!("rename orchestration result {} -> {}", temp.display(), result_path.display())
            }),
        }
    }

    async fn pending_result(&self, delivery_id: uuid::Uuid) -> Result<Option<TaskResult>> {
        let path = self.result_path(delivery_id);
        if !path_exists(&path).await? {
            return Ok(None);
        }
        let bytes =
            fs::read(&path).await.with_context(|| format!("read pending orchestration result {}", path.display()))?;
        let result = serde_json::from_slice(&bytes)
            .with_context(|| format!("decode pending orchestration result {}", path.display()))?;
        Ok(Some(result))
    }

    async fn pending_results(&self) -> Result<Vec<TaskResult>> {
        if !path_exists(&self.results_dir).await? {
            return Ok(Vec::new());
        }

        let mut paths = Vec::new();
        let mut dir = fs::read_dir(&self.results_dir).await.context("read orchestration result outbox dir")?;
        while let Some(entry) = dir.next_entry().await.context("iterate orchestration result outbox dir")? {
            if entry.path().extension().is_some_and(|ext| ext == "json") {
                paths.push(entry.path());
            }
        }
        paths.sort();

        let mut out = Vec::with_capacity(paths.len());
        for path in paths {
            let bytes = fs::read(&path)
                .await
                .with_context(|| format!("read pending orchestration result {}", path.display()))?;
            let result = serde_json::from_slice(&bytes)
                .with_context(|| format!("decode pending orchestration result {}", path.display()))?;
            out.push(result);
        }
        Ok(out)
    }

    async fn mark_completed(&self, delivery_id: uuid::Uuid) -> Result<()> {
        self.ensure_dirs().await?;

        let pending = self.pending_path(delivery_id);
        let result = self.result_path(delivery_id);
        let completed = self.completed_path(delivery_id);

        if path_exists(&completed).await? {
            if path_exists(&pending).await? {
                let _ = fs::remove_file(&pending).await;
            }
            if path_exists(&result).await? {
                let _ = fs::remove_file(&result).await;
            }
            return Ok(());
        }

        if path_exists(&result).await? {
            fs::remove_file(&result)
                .await
                .with_context(|| format!("remove completed orchestration result {}", result.display()))?;
            sync_dir(self.results_dir.clone()).await?;
        }

        if path_exists(&pending).await? {
            fs::rename(&pending, &completed)
                .await
                .with_context(|| format!("rename assignment inbox {} -> {}", pending.display(), completed.display()))?;
            sync_dir(self.pending_dir.clone()).await?;
            sync_dir(self.completed_dir.clone()).await?;
            return Ok(());
        }

        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&completed)
            .await
            .with_context(|| format!("create completed assignment tombstone {}", completed.display()))?;
        file.write_all(b"{}").await.context("write completed assignment tombstone")?;
        file.flush().await.context("flush completed assignment tombstone")?;
        file.sync_data().await.context("sync completed assignment tombstone")?;
        sync_dir(self.completed_dir.clone()).await?;
        Ok(())
    }

    async fn purge_completed_older_than(&self, max_age: Duration) -> Result<usize> {
        if !path_exists(&self.completed_dir).await? {
            return Ok(0);
        }

        let cutoff = SystemTime::now().checked_sub(max_age).unwrap_or(SystemTime::UNIX_EPOCH);
        let mut purged = 0;
        let mut dir = fs::read_dir(&self.completed_dir).await.context("read completed assignment inbox dir")?;
        while let Some(entry) = dir.next_entry().await.context("iterate completed assignment inbox dir")? {
            let path = entry.path();
            if path.extension().is_none_or(|ext| ext != "json") {
                continue;
            }
            let modified = entry
                .metadata()
                .await
                .with_context(|| format!("stat completed assignment tombstone {}", path.display()))?
                .modified()
                .with_context(|| format!("read mtime for completed assignment tombstone {}", path.display()))?;
            if modified <= cutoff {
                fs::remove_file(&path)
                    .await
                    .with_context(|| format!("remove expired completed assignment tombstone {}", path.display()))?;
                purged += 1;
            }
        }

        if purged > 0 {
            sync_dir(self.completed_dir.clone()).await?;
        }
        Ok(purged)
    }

    #[cfg(test)]
    async fn pending_count(&self) -> Result<usize> {
        if !path_exists(&self.pending_dir).await? {
            return Ok(0);
        }

        let mut count = 0;
        let mut dir = fs::read_dir(&self.pending_dir).await.context("read assignment inbox pending dir")?;
        while let Some(entry) = dir.next_entry().await.context("iterate assignment inbox pending dir")? {
            if entry.path().extension().is_some_and(|ext| ext == "json") {
                count += 1;
            }
        }
        Ok(count)
    }

    #[cfg(test)]
    async fn is_completed(&self, delivery_id: uuid::Uuid) -> Result<bool> {
        path_exists(&self.completed_path(delivery_id)).await.map_err(Into::into)
    }

    fn pending_path(&self, delivery_id: uuid::Uuid) -> PathBuf {
        self.pending_dir.join(format!("{delivery_id}.json"))
    }

    fn completed_path(&self, delivery_id: uuid::Uuid) -> PathBuf {
        self.completed_dir.join(format!("{delivery_id}.json"))
    }

    fn temp_path(&self, delivery_id: uuid::Uuid) -> PathBuf {
        self.pending_dir.join(format!("{delivery_id}.json.tmp"))
    }

    fn result_path(&self, delivery_id: uuid::Uuid) -> PathBuf {
        self.results_dir.join(format!("{delivery_id}.json"))
    }

    fn result_temp_path(&self, delivery_id: uuid::Uuid) -> PathBuf {
        self.results_dir.join(format!("{delivery_id}.json.tmp"))
    }

    async fn ensure_dirs(&self) -> Result<()> {
        fs::create_dir_all(&self.pending_dir)
            .await
            .with_context(|| format!("create assignment inbox dir {}", self.pending_dir.display()))?;
        fs::create_dir_all(&self.results_dir)
            .await
            .with_context(|| format!("create orchestration result outbox dir {}", self.results_dir.display()))?;
        fs::create_dir_all(&self.completed_dir)
            .await
            .with_context(|| format!("create assignment inbox dir {}", self.completed_dir.display()))?;
        Ok(())
    }
}

#[derive(Clone, Default, Debug)]
struct AssignmentExecutionGate {
    active: Arc<Mutex<HashSet<uuid::Uuid>>>,
}

impl AssignmentExecutionGate {
    async fn try_start(&self, delivery_id: uuid::Uuid) -> bool {
        self.active.lock().await.insert(delivery_id)
    }

    async fn finish(&self, delivery_id: uuid::Uuid) {
        self.active.lock().await.remove(&delivery_id);
    }
}

async fn path_exists(path: &Path) -> std::io::Result<bool> {
    match fs::metadata(path).await {
        Ok(_) => Ok(true),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(err) => Err(err),
    }
}

async fn sync_dir(path: PathBuf) -> Result<()> {
    tokio::task::spawn_blocking(move || -> std::io::Result<()> {
        let dir = std::fs::File::open(path)?;
        dir.sync_all()
    })
    .await
    .context("join assignment inbox directory sync task")?
    .context("sync assignment inbox directory")
}

/// Drives the `orchestration.assigned.<agent_id>` subscription.
pub struct OrchestrationSubscriber {
    client: Client,
    agent_id: String,
    hmac_key: Vec<u8>,
    cli_tool: String,
    cli_model: Option<String>,
    inbox: AssignmentInbox,
    execution_gate: AssignmentExecutionGate,
    result_subject_prefix: String,
}

impl OrchestrationSubscriber {
    pub fn new(
        client: Client,
        agent_id: String,
        hmac_secret: &str,
        cli_tool: String,
        cli_model: Option<String>,
        wal_path: Option<&str>,
    ) -> Self {
        Self {
            client,
            agent_id,
            hmac_key: hmac_secret.as_bytes().to_vec(),
            cli_tool,
            cli_model,
            inbox: AssignmentInbox::new(wal_path),
            execution_gate: AssignmentExecutionGate::default(),
            result_subject_prefix: RESULT_SUBJECT_PREFIX.to_string(),
        }
    }

    #[cfg(test)]
    fn with_result_subject_prefix(mut self, prefix: &str) -> Self {
        self.result_subject_prefix = prefix.to_string();
        self
    }

    /// Subscribe and dispatch until shutdown. Unparseable payloads are logged
    /// and dropped; a message that cannot even be logged is never retried
    /// because the platform emits assignments at-most-once and retrying a
    /// malformed payload would just spam.
    pub async fn run(self, mut shutdown: tokio::sync::watch::Receiver<bool>) {
        let agent_id = match uuid_from_agent(&self.agent_id) {
            Some(id) => id,
            None => {
                tracing::error!(agent_id = %self.agent_id, "Agent ID is not a valid UUID; orchestration subscriber disabled");
                return;
            }
        };
        let subject = assign_subject(agent_id);
        let durable = assignment_consumer_name(agent_id);
        let jetstream = jetstream::new(self.client.clone());

        let consumer: PullConsumer = match jetstream
            .create_consumer_on_stream(
                pull::Config {
                    durable_name: Some(durable.clone()),
                    name: Some(durable.clone()),
                    ack_policy: consumer::AckPolicy::Explicit,
                    ack_wait: Duration::from_secs(ASSIGNMENT_ACK_WAIT_SECS),
                    max_deliver: ASSIGNMENT_MAX_DELIVER,
                    filter_subject: subject.clone(),
                    ..Default::default()
                },
                ORCHESTRATION_ASSIGNMENTS_STREAM,
            )
            .await
        {
            Ok(consumer) => consumer,
            Err(err) => {
                tracing::error!(error = %err, durable = %durable, subject = %subject, "Failed to bind durable orchestration assignment consumer");
                return;
            }
        };

        tracing::info!(subject = %subject, durable = %durable, cli_tool = %self.cli_tool, "Orchestration subscriber listening");

        let handler = self.handler();
        if let Err(err) = handler.replay_pending().await {
            tracing::warn!(error = %err, durable = %durable, "Failed to replay pending orchestration assignments");
        }
        let mut replay_tick = tokio::time::interval(Duration::from_secs(ASSIGNMENT_REPLAY_INTERVAL_SECS));
        replay_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        replay_tick.tick().await;

        loop {
            tokio::select! {
                _ = shutdown.changed() => {
                    if *shutdown.borrow() {
                        tracing::info!("Orchestration subscriber shutting down");
                        break;
                    }
                }
                _ = replay_tick.tick() => {
                    if let Err(err) = handler.replay_pending().await {
                        tracing::warn!(error = %err, durable = %durable, "Failed to replay pending orchestration assignments");
                    }
                }
                batch = consumer.fetch().max_messages(ASSIGNMENT_FETCH_BATCH_SIZE).expires(Duration::from_millis(ASSIGNMENT_FETCH_TIMEOUT_MS)).messages() => {
                    match batch {
                        Ok(mut messages) => {
                            while let Some(message) = messages.next().await {
                                let Ok(message) = message else { break; };
                                let payload = message.payload.to_vec();
                                let reply_subject = message.reply.as_ref().map(|s| s.to_string());
                                match handler.dispatch(payload).await {
                                    DispatchAction::Ack => {
                                        if let Err(err) = message.ack().await {
                                            tracing::warn!(error = %err, reply_subject = ?reply_subject, "Failed to ack orchestration assignment");
                                        }
                                    }
                                    DispatchAction::Term => {
                                        tracing::warn!(reply_subject = ?reply_subject, "Dropping invalid orchestration assignment");
                                        let _ = message.ack_with(AckKind::Term).await;
                                    }
                                    DispatchAction::Nak => {
                                        tracing::warn!(reply_subject = ?reply_subject, "Retrying orchestration assignment after transient durable intake failure");
                                        let _ = message.ack_with(AckKind::Nak(None)).await;
                                    }
                                }
                            }
                        }
                        Err(err) => {
                            tracing::warn!(error = %err, durable = %durable, "Failed to fetch orchestration assignments");
                            tokio::time::sleep(Duration::from_secs(1)).await;
                        }
                    }
                }
            }
        }
    }

    fn handler(&self) -> AssignmentHandler {
        AssignmentHandler {
            client: self.client.clone(),
            agent_id: self.agent_id.clone(),
            hmac_key: self.hmac_key.clone(),
            cli_tool: self.cli_tool.clone(),
            cli_model: self.cli_model.clone(),
            inbox: self.inbox.clone(),
            execution_gate: self.execution_gate.clone(),
            result_subject_prefix: self.result_subject_prefix.clone(),
        }
    }
}

/// Self-contained handler cloned into each spawned task.
#[derive(Clone)]
struct AssignmentHandler {
    client: Client,
    agent_id: String,
    hmac_key: Vec<u8>,
    cli_tool: String,
    cli_model: Option<String>,
    inbox: AssignmentInbox,
    execution_gate: AssignmentExecutionGate,
    result_subject_prefix: String,
}

impl AssignmentHandler {
    async fn dispatch(&self, payload: Vec<u8>) -> DispatchAction {
        let assignment = match serde_json::from_slice::<TaskAssignment>(&payload) {
            Ok(assignment) => assignment,
            Err(err) => {
                tracing::warn!(error = %err, "Dropping malformed orchestration assignment");
                return DispatchAction::Term;
            }
        };

        // Cross-check the assigned agent_id matches this sidecar so a stray
        // subject wildcard can't hand another container's work to us.
        if assignment.agent_id.to_string() != self.agent_id {
            tracing::warn!(
                expected_agent_id = %self.agent_id,
                actual_agent_id = %assignment.agent_id,
                task_id = %assignment.task_id,
                "Dropping orchestration assignment addressed to a different agent"
            );
            return DispatchAction::Term;
        }

        if let Err(reason) = validate_durable_assignment(&assignment) {
            tracing::warn!(
                task_id = %assignment.task_id,
                %reason,
                "Dropping orchestration assignment that is missing durable delivery metadata"
            );
            return DispatchAction::Term;
        }

        match self.inbox.accept(&assignment).await {
            Ok(AssignmentInboxState::Accepted) => {
                tracing::info!(
                    task_id = %assignment.task_id,
                    delivery_id = ?assignment.delivery_id,
                    "Accepted orchestration assignment into durable inbox"
                );
                self.schedule_assignment(assignment).await;
                DispatchAction::Ack
            }
            Ok(AssignmentInboxState::Pending) => {
                tracing::info!(
                    task_id = %assignment.task_id,
                    delivery_id = ?assignment.delivery_id,
                    "Orchestration assignment already durably accepted; ensuring replay is scheduled"
                );
                self.schedule_assignment(assignment).await;
                DispatchAction::Ack
            }
            Ok(AssignmentInboxState::Completed) => {
                tracing::info!(
                    task_id = %assignment.task_id,
                    delivery_id = ?assignment.delivery_id,
                    "Ignoring duplicate orchestration assignment after completed tombstone"
                );
                DispatchAction::Ack
            }
            Err(err) => {
                tracing::warn!(
                    error = %err,
                    task_id = %assignment.task_id,
                    delivery_id = ?assignment.delivery_id,
                    "Failed to persist orchestration assignment intake"
                );
                DispatchAction::Nak
            }
        }
    }

    async fn replay_pending(&self) -> Result<()> {
        self.replay_ready_results().await?;

        let pending = self.inbox.pending_assignments().await?;
        if !pending.is_empty() {
            tracing::info!(count = pending.len(), "Replaying pending orchestration assignments from durable inbox");
        }
        for assignment in pending {
            if assignment.agent_id.to_string() != self.agent_id {
                tracing::warn!(
                    expected_agent_id = %self.agent_id,
                    actual_agent_id = %assignment.agent_id,
                    task_id = %assignment.task_id,
                    delivery_id = ?assignment.delivery_id,
                    "Skipping pending orchestration assignment for a different agent"
                );
                continue;
            }
            if let Err(reason) = validate_durable_assignment(&assignment) {
                tracing::warn!(
                    task_id = %assignment.task_id,
                    delivery_id = ?assignment.delivery_id,
                    %reason,
                    "Dropping pending orchestration assignment that is missing durable delivery metadata"
                );
                if let Some(delivery_id) = assignment.delivery_id
                    && let Err(err) = self.inbox.mark_completed(delivery_id).await
                {
                    tracing::warn!(
                        error = %err,
                        %delivery_id,
                        "Failed to tombstone invalid pending orchestration assignment"
                    );
                }
                continue;
            }
            self.schedule_assignment(assignment).await;
        }

        let purged =
            self.inbox.purge_completed_older_than(Duration::from_secs(COMPLETED_TOMBSTONE_MAX_AGE_SECS)).await?;
        if purged > 0 {
            tracing::info!(purged, "Purged expired completed orchestration assignment tombstones");
        }
        Ok(())
    }

    async fn replay_ready_results(&self) -> Result<()> {
        let results = self.inbox.pending_results().await?;
        if !results.is_empty() {
            tracing::info!(count = results.len(), "Replaying pending orchestration results from durable outbox");
        }
        for result in results {
            self.schedule_stored_result(result).await;
        }
        Ok(())
    }

    async fn schedule_stored_result(&self, result: TaskResult) {
        let Some(delivery_id) = result.delivery_id else {
            tracing::warn!(task_id = %result.task_id, "Skipping persisted orchestration result without delivery_id");
            return;
        };
        if !self.execution_gate.try_start(delivery_id).await {
            tracing::debug!(%delivery_id, task_id = %result.task_id, "Orchestration result already being published");
            return;
        }

        let handler = self.clone();
        tokio::spawn(async move {
            if let Err(err) = handler.publish_stored_result(result.clone()).await {
                tracing::warn!(
                    error = %err,
                    task_id = %result.task_id,
                    delivery_id = ?result.delivery_id,
                    "Orchestration result publish will rely on durable outbox replay"
                );
            }
            handler.execution_gate.finish(delivery_id).await;
        });
    }

    async fn schedule_assignment(&self, assignment: TaskAssignment) {
        let delivery_id = assignment.delivery_id;
        if let Some(delivery_id) = delivery_id
            && !self.execution_gate.try_start(delivery_id).await
        {
            tracing::debug!(%delivery_id, task_id = %assignment.task_id, "Orchestration assignment already running");
            return;
        }

        let handler = self.clone();
        tokio::spawn(async move {
            let result = match delivery_id {
                Some(delivery_id) => match handler.inbox.pending_result(delivery_id).await {
                    Ok(Some(result)) => handler.publish_stored_result(result).await,
                    Ok(None) => handler.execute_assignment(assignment.clone()).await,
                    Err(err) => Err(err),
                },
                None => Err(anyhow::anyhow!("orchestration assignment missing delivery_id")),
            };
            if let Err(err) = result {
                tracing::warn!(
                    error = %err,
                    task_id = %assignment.task_id,
                    delivery_id = ?assignment.delivery_id,
                    "Orchestration assignment execution will rely on durable inbox replay"
                );
            }
            if let Some(delivery_id) = delivery_id {
                handler.execution_gate.finish(delivery_id).await;
            }
        });
    }

    async fn execute_assignment(&self, assignment: TaskAssignment) -> Result<()> {
        tracing::info!(task_id = %assignment.task_id, delivery_id = ?assignment.delivery_id, "Running orchestration task");

        let outcome = run_cli(&self.cli_tool, self.cli_model.as_deref(), &assignment).await;
        let result = TaskResult {
            delivery_id: assignment.delivery_id,
            attempt: assignment.attempt,
            task_id: assignment.task_id,
            agent_id: assignment.agent_id,
            outcome,
        };

        result.delivery_id.context("orchestration result missing delivery_id")?;
        self.inbox.store_result(&result).await?;
        self.publish_stored_result(result).await?;

        Ok(())
    }

    async fn publish_stored_result(&self, result: TaskResult) -> Result<()> {
        let delivery_id = result.delivery_id.context("stored orchestration result missing delivery_id")?;
        self.publish_result(result).await?;
        self.inbox.mark_completed(delivery_id).await?;
        Ok(())
    }

    async fn publish_result(&self, result: TaskResult) -> Result<()> {
        let subject = result_subject_for(&self.result_subject_prefix, result.agent_id);
        let timestamp = Utc::now().timestamp();
        let envelope = SignedEnvelope::sign(&self.hmac_key, &self.agent_id, timestamp, &result)
            .context("sign orchestration result envelope")?;
        let bytes = serde_json::to_vec(&envelope).context("serialize orchestration result envelope")?;
        let jetstream = jetstream::new(self.client.clone());
        jetstream
            .publish(subject.clone(), bytes.into())
            .await
            .with_context(|| format!("publish orchestration result {} to {subject}", result.task_id))?
            .await
            .with_context(|| format!("await orchestration result ack {} on {subject}", result.task_id))?;
        Ok(())
    }
}

fn result_subject_for(prefix: &str, agent_id: uuid::Uuid) -> String {
    format!("{prefix}.{agent_id}")
}

/// Construct the CLI command for a tool. Non-interactive print modes are used
/// so the process exits once the task is complete.
///
/// Returns `None` when the tool is unsupported so the caller can fail the task
/// with a useful stderr instead of panicking.
fn cli_command(cli_tool: &str, cli_model: Option<&str>, prompt: &str) -> Option<Command> {
    let mut cmd = match CliToolKind::parse_legacy(cli_tool).ok()? {
        CliToolKind::Claude => {
            let mut c = Command::new("claude");
            c.args(["-p", prompt, "--dangerously-skip-permissions"]);
            if let Some(model) = cli_model {
                c.args(["--model", model]);
            }
            c
        }
        CliToolKind::Codex => {
            let mut c = Command::new("codex");
            c.args([
                "exec",
                "--dangerously-bypass-approvals-and-sandbox",
                "--skip-git-repo-check",
                "--ephemeral",
                "--ignore-user-config",
                "--ignore-rules",
                "--color",
                "never",
            ]);
            if let Some(model) = cli_model {
                c.args(["--model", model]);
            }
            c.arg(prompt);
            c
        }
        CliToolKind::Gemini => {
            let mut c = Command::new("gemini");
            c.args(["-p", prompt]);
            c
        }
        CliToolKind::Opencode => {
            let mut c = Command::new("opencode");
            c.args(["run", prompt]);
            c
        }
    };
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped()).stdin(Stdio::null());
    Some(cmd)
}

/// Assemble the prompt the CLI sees from an assignment. The structured
/// `task`/`message` pair is flattened because all four wrapped CLIs take a
/// single prompt argument.
fn build_prompt(assignment: &TaskAssignment) -> String {
    if assignment.message.is_empty() {
        assignment.task.clone()
    } else {
        format!("{}\n\n{}", assignment.task, assignment.message)
    }
}

/// Max bytes of stdout/stderr carried back in a TaskOutcome. NATS's default
/// `max_payload` is 1 MiB; a verbose CLI can blow past that and the publish
/// silently fails, leaving the task stuck at `working`. 256 KiB leaves
/// headroom for the envelope, signature, and protocol fields.
const MAX_OUTPUT_BYTES: usize = 256 * 1024;

fn truncate_output(s: String) -> String {
    if s.len() <= MAX_OUTPUT_BYTES {
        return s;
    }
    // Trim to the nearest char boundary at or below the limit to avoid
    // slicing a multi-byte UTF-8 sequence.
    let mut end = MAX_OUTPUT_BYTES;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    let mut out = s[..end].to_string();
    out.push_str("\n... [truncated — output exceeded 256KB]");
    out
}

async fn run_cli(cli_tool: &str, cli_model: Option<&str>, assignment: &TaskAssignment) -> TaskOutcome {
    if let Err(err) = apply_context_envelope(cli_tool, assignment).await {
        tracing::warn!(
            error = %err,
            task_id = %assignment.task_id,
            delivery_id = ?assignment.delivery_id,
            "Context envelope adapter failed; continuing without injected context"
        );
    }

    let prompt = build_prompt(assignment);
    let Some(mut cmd) = cli_command(cli_tool, cli_model, &prompt) else {
        return TaskOutcome::Failed { stderr: format!("unsupported cli_tool '{cli_tool}'"), exit_code: None };
    };
    let Some(timeout) = cli_timeout_for_assignment(assignment) else {
        return TaskOutcome::Failed {
            stderr: "assignment lease expired before CLI execution started".to_string(),
            exit_code: None,
        };
    };
    cmd.kill_on_drop(true);

    match tokio::time::timeout(timeout, cmd.output()).await {
        Ok(Ok(output)) if output.status.success() => {
            TaskOutcome::Completed { stdout: truncate_output(String::from_utf8_lossy(&output.stdout).into_owned()) }
        }
        Ok(Ok(output)) => TaskOutcome::Failed {
            stderr: truncate_output(String::from_utf8_lossy(&output.stderr).into_owned()),
            exit_code: output.status.code(),
        },
        Ok(Err(err)) => TaskOutcome::Failed { stderr: format!("failed to spawn {cli_tool}: {err}"), exit_code: None },
        Err(_) => TaskOutcome::Failed {
            stderr: format!("cli_tool '{cli_tool}' exceeded assignment lease timeout ({}s)", timeout.as_secs()),
            exit_code: None,
        },
    }
}

async fn apply_context_envelope(cli_tool: &str, assignment: &TaskAssignment) -> Result<()> {
    if !context_injection_enabled(std::env::var("AGENTFORGE_CONTEXT_INJECTION_ENABLED").ok().as_deref()) {
        return Ok(());
    }
    let Some(envelope) = &assignment.context_envelope else {
        return Ok(());
    };
    let Some(adapter) = context_adapter_for_cli_tool(cli_tool) else {
        return Ok(());
    };

    let context_dir = PathBuf::from("/tmp/agentforge-context");
    fs::create_dir_all(&context_dir).await.context("create context envelope temp dir")?;
    let stem = assignment.delivery_id.unwrap_or(assignment.task_id);
    let envelope_path = context_dir.join(format!("{stem}.json"));
    let report_path = context_dir.join(format!("{stem}.report.json"));
    let bytes = serde_json::to_vec(envelope).context("serialize context envelope")?;
    fs::write(&envelope_path, bytes)
        .await
        .with_context(|| format!("write context envelope {}", envelope_path.display()))?;

    let home = std::env::var("HOME").unwrap_or_else(|_| "/home/agent".to_string());
    let envelope_arg = envelope_path.to_string_lossy().into_owned();
    let report_arg = report_path.to_string_lossy().into_owned();
    let mut cmd = Command::new("agent-context-helper");
    cmd.args([
        "--adapter",
        adapter,
        "--envelope",
        envelope_arg.as_str(),
        "--home",
        home.as_str(),
        "--report",
        report_arg.as_str(),
    ]);

    let output = tokio::time::timeout(Duration::from_secs(10), cmd.output())
        .await
        .context("context helper timed out")?
        .context("spawn context helper")?;
    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    Err(anyhow::anyhow!("context helper exited with {}: {}", output.status, stderr.trim()))
}

fn context_injection_enabled(raw: Option<&str>) -> bool {
    matches!(raw.map(str::trim).map(str::to_ascii_lowercase).as_deref(), Some("1" | "true" | "yes" | "on"))
}

fn context_adapter_for_cli_tool(cli_tool: &str) -> Option<&'static str> {
    match CliToolKind::parse_legacy(cli_tool).ok()? {
        CliToolKind::Claude => Some("claude"),
        CliToolKind::Codex => Some("codex"),
        CliToolKind::Gemini => Some("gemini"),
        CliToolKind::Opencode => Some("opencode"),
    }
}

fn cli_timeout_for_assignment(assignment: &TaskAssignment) -> Option<Duration> {
    let Some(deadline) = assignment.lease_expires_at else {
        return Some(Duration::from_secs(DEFAULT_CLI_TIMEOUT_SECS));
    };
    let remaining = deadline.signed_duration_since(Utc::now());
    remaining.to_std().ok().filter(|duration| !duration.is_zero())
}

fn validate_durable_assignment(assignment: &TaskAssignment) -> Result<(), &'static str> {
    if assignment.delivery_id.is_none() {
        return Err("missing_delivery_id");
    }
    if assignment.attempt.is_none() {
        return Err("missing_attempt");
    }
    if assignment.lease_expires_at.is_none() {
        return Err("missing_lease_expires_at");
    }
    Ok(())
}

fn uuid_from_agent(agent_id: &str) -> Option<uuid::Uuid> {
    uuid::Uuid::parse_str(agent_id).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentforge_core::orchestration_protocol::assign_subject_wildcard;
    use async_nats::jetstream::stream;
    use std::collections::HashMap;
    use std::fs as std_fs;
    use tokio::sync::watch;
    use tokio::time::timeout;
    use uuid::Uuid;

    const TEST_HMAC: &str = "sidecar-assignment-contract-hmac";

    fn sample_assignment() -> TaskAssignment {
        TaskAssignment {
            delivery_id: Some(Uuid::now_v7()),
            attempt: Some(1),
            lease_expires_at: Some(Utc::now()),
            task_id: Uuid::now_v7(),
            agent_id: Uuid::now_v7(),
            title: "Sweep".into(),
            task: "Do a thing".into(),
            message: String::new(),
            priority: "normal".into(),
            context_envelope: None,
        }
    }

    #[test]
    fn context_injection_flag_defaults_off() {
        assert!(!context_injection_enabled(None));
        assert!(!context_injection_enabled(Some("false")));
        assert!(!context_injection_enabled(Some("0")));
        assert!(context_injection_enabled(Some("true")));
        assert!(context_injection_enabled(Some("1")));
    }

    #[test]
    fn context_adapter_mapping_includes_codex_and_gemini_follow_ons() {
        assert_eq!(context_adapter_for_cli_tool("claude"), Some("claude"));
        assert_eq!(context_adapter_for_cli_tool("codex"), Some("codex"));
        assert_eq!(context_adapter_for_cli_tool("gemini"), Some("gemini"));
        assert_eq!(context_adapter_for_cli_tool("opencode"), Some("opencode"));
        assert_eq!(context_adapter_for_cli_tool("vim"), None);
    }

    fn completed_result_for(assignment: &TaskAssignment, stdout: &str) -> TaskResult {
        TaskResult {
            delivery_id: assignment.delivery_id,
            attempt: assignment.attempt,
            task_id: assignment.task_id,
            agent_id: assignment.agent_id,
            outcome: TaskOutcome::Completed { stdout: stdout.to_string() },
        }
    }

    async fn try_connect() -> Option<async_nats::Client> {
        let candidates = nats_candidates();
        let mut failures = Vec::new();
        for (label, url) in candidates {
            match timeout(Duration::from_millis(500), agentforge_infra::nats::connect_nats(&url)).await {
                Ok(Ok(client)) => return Some(client),
                Ok(Err(err)) => failures.push(format!("{label}: {err}")),
                Err(_) => failures.push(format!("{label}: timeout")),
            }
        }
        eprintln!("skipping: failed to connect to project NATS ({})", failures.join("; "));
        None
    }

    fn nats_candidates() -> Vec<(String, String)> {
        let mut out = Vec::new();
        let mut seen = HashSet::new();
        let docker_env = read_docker_env();

        let mut push = |label: String, url: String| {
            if seen.insert(url.clone()) {
                out.push((label, url));
            }
        };

        if let Ok(url) = std::env::var("NATS_URL") {
            push("env:NATS_URL".to_string(), url);
        }
        if let Some(url) = docker_env.get("NATS_URL").cloned() {
            push("docker/.env:NATS_URL".to_string(), url);
        }

        let port = std::env::var("NATS_PORT")
            .ok()
            .or_else(|| docker_env.get("NATS_PORT").cloned())
            .unwrap_or_else(|| "4222".to_string());
        if let Some(password) =
            std::env::var("NATS_BACKEND_PASSWORD").ok().or_else(|| docker_env.get("NATS_BACKEND_PASSWORD").cloned())
        {
            push("docker/.env backend user".to_string(), format!("nats://backend:{password}@127.0.0.1:{port}"));
        }

        push("localhost anonymous".to_string(), format!("nats://127.0.0.1:{port}"));
        out
    }

    fn read_docker_env() -> HashMap<String, String> {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../docker/.env");
        let Ok(contents) = std_fs::read_to_string(path) else {
            return HashMap::new();
        };
        contents
            .lines()
            .filter_map(|line| {
                let line = line.trim();
                if line.is_empty() || line.starts_with('#') {
                    return None;
                }
                let (key, value) = line.split_once('=')?;
                Some((key.trim().to_string(), value.trim().to_string()))
            })
            .collect()
    }

    fn test_result_subject_prefix() -> String {
        format!("orchestration.result.sidecar-contract.{}", Uuid::now_v7().simple())
    }

    fn test_results_stream_name(result_prefix: &str) -> String {
        let suffix = result_prefix.rsplit('.').next().unwrap_or("unknown");
        format!("ORCH_SIDECAR_RESULTS_{suffix}")
    }

    async fn ensure_live_streams(client: async_nats::Client, result_stream_name: &str, result_prefix: &str) {
        let js = jetstream::new(client);
        js.create_or_update_stream(stream::Config {
            name: ORCHESTRATION_ASSIGNMENTS_STREAM.to_string(),
            subjects: vec![assign_subject_wildcard()],
            retention: stream::RetentionPolicy::WorkQueue,
            storage: stream::StorageType::File,
            max_age: Duration::from_secs(24 * 60 * 60),
            discard: stream::DiscardPolicy::Old,
            max_messages_per_subject: 1_000,
            ..Default::default()
        })
        .await
        .expect("ensure assignments stream");
        js.create_or_update_stream(stream::Config {
            name: result_stream_name.to_string(),
            subjects: vec![format!("{result_prefix}.*")],
            retention: stream::RetentionPolicy::WorkQueue,
            storage: stream::StorageType::File,
            max_age: Duration::from_secs(24 * 60 * 60),
            discard: stream::DiscardPolicy::Old,
            max_messages_per_subject: 1_000,
            ..Default::default()
        })
        .await
        .expect("ensure results stream");
    }

    async fn cleanup_test_results_stream(client: async_nats::Client, result_stream_name: &str) {
        let js = jetstream::new(client);
        let _ = js.delete_stream(result_stream_name).await;
    }

    async fn next_result(subscriber: &mut async_nats::Subscriber, wait: Duration) -> Option<TaskResult> {
        let message = timeout(wait, subscriber.next()).await.ok()??;
        let envelope: SignedEnvelope = serde_json::from_slice(&message.payload).expect("decode result envelope");
        Some(serde_json::from_value(envelope.payload).expect("decode result payload"))
    }

    async fn wait_until_completed(inbox: &AssignmentInbox, delivery_id: Uuid) {
        for _ in 0..40 {
            if inbox.pending_count().await.expect("pending count") == 0
                && inbox.is_completed(delivery_id).await.expect("completed tombstone")
            {
                return;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        panic!("assignment inbox did not move delivery to completed");
    }

    async fn stop_live_subscriber(
        client: async_nats::Client,
        agent_id: Uuid,
        shutdown_tx: watch::Sender<bool>,
        handle: tokio::task::JoinHandle<()>,
    ) {
        let _ = shutdown_tx.send(true);
        let _ = timeout(Duration::from_secs(2), handle).await;
        if let Ok(stream) = jetstream::new(client).get_stream(ORCHESTRATION_ASSIGNMENTS_STREAM).await {
            let _ = stream.delete_consumer(&assignment_consumer_name(agent_id)).await;
        }
    }

    #[test]
    fn build_prompt_drops_empty_message() {
        let mut a = sample_assignment();
        a.message = String::new();
        assert_eq!(build_prompt(&a), "Do a thing");
    }

    #[test]
    fn build_prompt_concatenates_task_and_message() {
        let mut a = sample_assignment();
        a.message = "with context".into();
        assert_eq!(build_prompt(&a), "Do a thing\n\nwith context");
    }

    #[test]
    fn cli_command_supports_known_tools() {
        for tool in CliToolKind::ALL.map(CliToolKind::as_str) {
            assert!(cli_command(tool, Some("test-model"), "hello").is_some(), "{tool} must be supported");
        }
    }

    #[test]
    fn cli_command_returns_none_for_unknown_tool() {
        assert!(cli_command("fake-cli", None, "hello").is_none());
    }

    #[tokio::test]
    async fn run_cli_reports_unsupported_tool_without_panic() {
        // Unsupported cli_tool value must turn into a Failed outcome with a
        // clear stderr rather than a panic.
        let assignment = sample_assignment();
        let outcome = run_cli("fake-cli", None, &assignment).await;
        match outcome {
            TaskOutcome::Failed { stderr, exit_code } => {
                assert!(stderr.contains("unsupported cli_tool"), "stderr = {stderr}");
                assert!(exit_code.is_none());
            }
            TaskOutcome::Completed { .. } => panic!("expected Failed"),
        }
    }

    #[test]
    fn truncate_output_passes_small_strings_through() {
        let s = "hello".to_string();
        assert_eq!(truncate_output(s.clone()), s);
    }

    #[test]
    fn truncate_output_caps_large_strings() {
        let s = "a".repeat(MAX_OUTPUT_BYTES + 1024);
        let out = truncate_output(s);
        assert!(out.ends_with("[truncated — output exceeded 256KB]"));
        assert!(out.len() <= MAX_OUTPUT_BYTES + 100);
    }

    #[test]
    fn truncate_output_respects_utf8_boundaries() {
        // Construct a string where the cutoff would otherwise land mid-codepoint.
        let filler = "a".repeat(MAX_OUTPUT_BYTES - 1);
        let s = format!("{filler}漢"); // '漢' is 3 bytes; cutoff at MAX lands mid-char
        let out = truncate_output(s);
        assert!(out.is_char_boundary(out.len())); // full string is valid UTF-8
        assert!(out.ends_with("[truncated — output exceeded 256KB]"));
    }

    #[tokio::test]
    async fn assignment_inbox_transitions_from_pending_to_completed() {
        let tmp = tempfile::tempdir().unwrap();
        let inbox = AssignmentInbox::new(Some(tmp.path().to_str().unwrap()));
        let assignment = sample_assignment();
        let delivery_id = assignment.delivery_id.expect("delivery_id");

        assert_eq!(inbox.accept(&assignment).await.unwrap(), AssignmentInboxState::Accepted);
        assert_eq!(inbox.pending_count().await.unwrap(), 1);

        assert_eq!(inbox.accept(&assignment).await.unwrap(), AssignmentInboxState::Pending);

        inbox.mark_completed(delivery_id).await.unwrap();
        assert_eq!(inbox.pending_count().await.unwrap(), 0);
        assert!(inbox.is_completed(delivery_id).await.unwrap());

        assert_eq!(inbox.accept(&assignment).await.unwrap(), AssignmentInboxState::Completed);
    }

    #[tokio::test]
    async fn assignment_inbox_replay_lists_only_unfinished_deliveries() {
        let tmp = tempfile::tempdir().unwrap();
        let inbox = AssignmentInbox::new(Some(tmp.path().to_str().unwrap()));
        let first = sample_assignment();
        let second = sample_assignment();

        inbox.accept(&first).await.unwrap();
        inbox.accept(&second).await.unwrap();
        inbox.mark_completed(first.delivery_id.expect("first delivery")).await.unwrap();

        let replay = inbox.pending_assignments().await.unwrap();
        assert_eq!(replay.len(), 1);
        assert_eq!(replay[0].delivery_id, second.delivery_id);
        assert_eq!(replay[0].task_id, second.task_id);
    }

    #[tokio::test]
    async fn assignment_result_outbox_is_removed_only_after_completion() {
        let tmp = tempfile::tempdir().unwrap();
        let inbox = AssignmentInbox::new(Some(tmp.path().to_str().unwrap()));
        let assignment = sample_assignment();
        let delivery_id = assignment.delivery_id.expect("delivery_id");
        let result = completed_result_for(&assignment, "cached output");

        inbox.accept(&assignment).await.unwrap();
        inbox.store_result(&result).await.unwrap();

        let pending = inbox.pending_result(delivery_id).await.unwrap().expect("pending result");
        assert_eq!(pending, result);
        assert_eq!(inbox.pending_count().await.unwrap(), 1);

        inbox.mark_completed(delivery_id).await.unwrap();

        assert!(inbox.pending_result(delivery_id).await.unwrap().is_none());
        assert_eq!(inbox.pending_count().await.unwrap(), 0);
        assert!(inbox.is_completed(delivery_id).await.unwrap());
    }

    #[tokio::test]
    async fn assignment_inbox_purges_expired_completed_tombstones() {
        let tmp = tempfile::tempdir().unwrap();
        let inbox = AssignmentInbox::new(Some(tmp.path().to_str().unwrap()));
        let assignment = sample_assignment();
        let delivery_id = assignment.delivery_id.expect("delivery_id");

        inbox.accept(&assignment).await.unwrap();
        inbox.mark_completed(delivery_id).await.unwrap();
        tokio::time::sleep(Duration::from_millis(5)).await;

        let purged = inbox.purge_completed_older_than(Duration::ZERO).await.unwrap();
        assert_eq!(purged, 1);
        assert!(!inbox.is_completed(delivery_id).await.unwrap());
    }

    #[test]
    fn cli_timeout_defaults_when_assignment_has_no_lease() {
        let mut assignment = sample_assignment();
        assignment.lease_expires_at = None;

        assert_eq!(cli_timeout_for_assignment(&assignment), Some(Duration::from_secs(DEFAULT_CLI_TIMEOUT_SECS)));
    }

    #[test]
    fn cli_timeout_rejects_expired_assignment_lease() {
        let mut assignment = sample_assignment();
        assignment.lease_expires_at = Some(Utc::now() - chrono::Duration::seconds(1));

        assert_eq!(cli_timeout_for_assignment(&assignment), None);
    }

    #[test]
    fn cli_timeout_uses_remaining_assignment_lease() {
        let mut assignment = sample_assignment();
        assignment.lease_expires_at = Some(Utc::now() + chrono::Duration::seconds(60));

        let timeout = cli_timeout_for_assignment(&assignment).expect("future lease should produce timeout");
        assert!(timeout <= Duration::from_secs(60));
        assert!(timeout > Duration::from_secs(55));
    }

    #[test]
    fn durable_assignment_validation_requires_delivery_metadata() {
        let mut assignment = sample_assignment();
        assert!(validate_durable_assignment(&assignment).is_ok());

        assignment.delivery_id = None;
        assert_eq!(validate_durable_assignment(&assignment), Err("missing_delivery_id"));

        assignment = sample_assignment();
        assignment.attempt = None;
        assert_eq!(validate_durable_assignment(&assignment), Err("missing_attempt"));

        assignment = sample_assignment();
        assignment.lease_expires_at = None;
        assert_eq!(validate_durable_assignment(&assignment), Err("missing_lease_expires_at"));
    }

    #[tokio::test]
    async fn assignment_execution_gate_allows_only_one_active_run_per_delivery() {
        let gate = AssignmentExecutionGate::default();
        let delivery_id = Uuid::now_v7();

        assert!(gate.try_start(delivery_id).await);
        assert!(!gate.try_start(delivery_id).await);

        gate.finish(delivery_id).await;
        assert!(gate.try_start(delivery_id).await);
    }

    #[tokio::test]
    async fn subscriber_replays_pending_assignment_from_durable_inbox() {
        let Some(client) = try_connect().await else {
            return;
        };
        let result_prefix = test_result_subject_prefix();
        let result_stream_name = test_results_stream_name(&result_prefix);
        ensure_live_streams(client.clone(), &result_stream_name, &result_prefix).await;

        let agent_id = Uuid::now_v7();
        let tmp = tempfile::tempdir().unwrap();
        let wal_path = tmp.path().to_str().unwrap().to_string();
        let inbox = AssignmentInbox::new(Some(&wal_path));
        let mut assignment = sample_assignment();
        assignment.agent_id = agent_id;
        let delivery_id = assignment.delivery_id.expect("delivery_id");

        assert_eq!(inbox.accept(&assignment).await.unwrap(), AssignmentInboxState::Accepted);

        let mut result_sub =
            client.subscribe(result_subject_for(&result_prefix, agent_id)).await.expect("subscribe results");
        tokio::time::sleep(Duration::from_millis(100)).await;

        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let subscriber = OrchestrationSubscriber::new(
            client.clone(),
            agent_id.to_string(),
            TEST_HMAC,
            "fake-cli".to_string(),
            None,
            Some(&wal_path),
        )
        .with_result_subject_prefix(&result_prefix);
        let handle = tokio::spawn(async move { subscriber.run(shutdown_rx).await });

        let result = next_result(&mut result_sub, Duration::from_secs(3)).await.expect("pending assignment result");
        assert_eq!(result.delivery_id, Some(delivery_id));
        assert_eq!(result.task_id, assignment.task_id);
        match result.outcome {
            TaskOutcome::Failed { stderr, exit_code } => {
                assert!(stderr.contains("unsupported cli_tool"), "stderr = {stderr}");
                assert!(exit_code.is_none());
            }
            TaskOutcome::Completed { stdout } => panic!("expected fake-cli failure, stdout = {stdout}"),
        }
        wait_until_completed(&inbox, delivery_id).await;

        stop_live_subscriber(client.clone(), agent_id, shutdown_tx, handle).await;
        cleanup_test_results_stream(client, &result_stream_name).await;
    }

    #[tokio::test]
    async fn subscriber_replays_persisted_result_without_rerunning_cli() {
        let Some(client) = try_connect().await else {
            return;
        };
        let result_prefix = test_result_subject_prefix();
        let result_stream_name = test_results_stream_name(&result_prefix);
        ensure_live_streams(client.clone(), &result_stream_name, &result_prefix).await;

        let agent_id = Uuid::now_v7();
        let tmp = tempfile::tempdir().unwrap();
        let wal_path = tmp.path().to_str().unwrap().to_string();
        let inbox = AssignmentInbox::new(Some(&wal_path));
        let mut assignment = sample_assignment();
        assignment.agent_id = agent_id;
        let delivery_id = assignment.delivery_id.expect("delivery_id");
        let result = completed_result_for(&assignment, "cached result from local outbox");

        inbox.accept(&assignment).await.unwrap();
        inbox.store_result(&result).await.unwrap();

        let mut result_sub =
            client.subscribe(result_subject_for(&result_prefix, agent_id)).await.expect("subscribe results");
        tokio::time::sleep(Duration::from_millis(100)).await;

        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let subscriber = OrchestrationSubscriber::new(
            client.clone(),
            agent_id.to_string(),
            TEST_HMAC,
            "fake-cli".to_string(),
            None,
            Some(&wal_path),
        )
        .with_result_subject_prefix(&result_prefix);
        let handle = tokio::spawn(async move { subscriber.run(shutdown_rx).await });

        let published =
            next_result(&mut result_sub, Duration::from_secs(3)).await.expect("persisted result should be replayed");
        assert_eq!(published.delivery_id, Some(delivery_id));
        match published.outcome {
            TaskOutcome::Completed { stdout } => assert_eq!(stdout, "cached result from local outbox"),
            TaskOutcome::Failed { stderr, .. } => panic!("result replay reran fake CLI: {stderr}"),
        }
        wait_until_completed(&inbox, delivery_id).await;

        stop_live_subscriber(client.clone(), agent_id, shutdown_tx, handle).await;
        cleanup_test_results_stream(client, &result_stream_name).await;
    }

    #[tokio::test]
    async fn completed_assignment_duplicate_is_not_executed_again() {
        let Some(client) = try_connect().await else {
            return;
        };
        let result_prefix = test_result_subject_prefix();
        let result_stream_name = test_results_stream_name(&result_prefix);
        ensure_live_streams(client.clone(), &result_stream_name, &result_prefix).await;

        let agent_id = Uuid::now_v7();
        let tmp = tempfile::tempdir().unwrap();
        let wal_path = tmp.path().to_str().unwrap().to_string();
        let inbox = AssignmentInbox::new(Some(&wal_path));
        let mut assignment = sample_assignment();
        assignment.agent_id = agent_id;
        let delivery_id = assignment.delivery_id.expect("delivery_id");

        inbox.accept(&assignment).await.unwrap();
        inbox.mark_completed(delivery_id).await.unwrap();

        let mut result_sub =
            client.subscribe(result_subject_for(&result_prefix, agent_id)).await.expect("subscribe results");
        tokio::time::sleep(Duration::from_millis(100)).await;

        let handler = AssignmentHandler {
            client,
            agent_id: agent_id.to_string(),
            hmac_key: TEST_HMAC.as_bytes().to_vec(),
            cli_tool: "fake-cli".to_string(),
            cli_model: None,
            inbox,
            execution_gate: AssignmentExecutionGate::default(),
            result_subject_prefix: result_prefix,
        };

        let action = handler.dispatch(serde_json::to_vec(&assignment).unwrap()).await;
        assert_eq!(action, DispatchAction::Ack);
        assert!(
            next_result(&mut result_sub, Duration::from_millis(700)).await.is_none(),
            "completed duplicate delivery must not publish a second result"
        );
        cleanup_test_results_stream(handler.client.clone(), &result_stream_name).await;
    }
}

//! CLI credential directory watcher (issue #41).
//!
//! Ports `platform/internal/sidecar/credentials.go` (commit `f0feb468`,
//! 2026-02-17). When the CLI tool inside the container writes a new
//! credentials file (e.g. `claude /login` → `~/.claude/credentials.json`)
//! this watcher:
//!
//! 1. fsnotify-observes the credentials directory (see
//!    `docker/scripts/agent-entrypoint.sh` for the per-CLI location).
//! 2. Debounces 500 ms to absorb tmp→rename burst writes.
//! 3. Reads every `.json` file under size caps
//!    (`MAX_CREDENTIAL_FILE_BYTES`, `MAX_CREDENTIAL_TOTAL_BYTES`,
//!    `MAX_CREDENTIAL_FILES` from `agentforge_core::credential_protocol`).
//! 4. Wraps the map in `CredentialSyncMessage` and HMAC-signs it with
//!    `SignedEnvelope::sign(hmac_secret, …)` so the backend consumer can
//!    verify. Same file contents produce the same HMAC envelope; the
//!    consumer upsert is idempotent. **Payload travels cleartext over
//!    NATS** (TLS + per-agent JWT provide transport auth + cross-tenant
//!    isolation). Consumer-side encryption at rest is done by
//!    `CliCredentialService` before the DB write.
//! 5. Publishes to `creds.<agent_id>` via JetStream. JetStream (not core
//!    NATS) because this payload must survive a backend restart between
//!    `claude /login` and the consumer waking back up.
//!
//! The backend `CredentialStreamWorker` is the counterpart.

use std::path::{Path, PathBuf};
use std::time::Duration;

use agentforge_core::credential_protocol::{
    CredentialSyncMessage, MAX_CREDENTIAL_FILE_BYTES, MAX_CREDENTIAL_FILES, MAX_CREDENTIAL_TOTAL_BYTES, creds_subject,
};
use agentforge_core::orchestration_protocol::SignedEnvelope;
use anyhow::{Context, Result, anyhow};
use async_nats::jetstream::{self, Context as JetStreamContext};
use notify::{EventKind, RecursiveMode, Watcher, recommended_watcher};
use tokio::sync::mpsc;
use tokio::sync::watch;
use uuid::Uuid;

/// Payload of `build_message` — `Empty` lets the caller no-op cleanly when
/// the directory has no credential files yet (watcher startup sweep).
#[derive(Debug)]
pub enum BuildOutcome {
    Message(CredentialSyncMessage),
    Empty,
}

/// Public entry point for the watcher. Runs until `shutdown` flips to `true`.
/// Returns `Err` only for unrecoverable setup failures (cannot create
/// watcher, cannot open JetStream context). Per-publish errors are logged
/// and the watcher keeps running.
pub async fn run(
    dir: PathBuf,
    client: async_nats::Client,
    agent_id: Uuid,
    organization_id: Uuid,
    cli_tool: String,
    hmac_secret: String,
    mut shutdown: watch::Receiver<bool>,
) -> Result<()> {
    tokio::fs::create_dir_all(&dir).await.with_context(|| format!("create creds dir {}", dir.display()))?;

    let js = jetstream::new(client);

    let (event_tx, mut event_rx) = mpsc::unbounded_channel::<()>();
    let mut watcher = recommended_watcher(move |res: Result<notify::Event, notify::Error>| match res {
        Ok(event) if matches!(event.kind, EventKind::Create(_) | EventKind::Modify(_)) => {
            if event.paths.iter().any(|p| is_credential_file(p)) {
                let _ = event_tx.send(());
            }
        }
        Ok(_) => {}
        Err(err) => tracing::warn!(error = %err, "notify error"),
    })
    .context("create notify watcher")?;

    watcher.watch(&dir, RecursiveMode::NonRecursive).with_context(|| format!("watch dir {}", dir.display()))?;

    tracing::info!(
        dir = %dir.display(),
        subject = %creds_subject(agent_id),
        %cli_tool,
        "credential watcher started"
    );

    let subject = creds_subject(agent_id);

    // Initial sweep so a container restarted with existing credentials
    // re-syncs on startup (cheap and idempotent — same file contents
    // produce the same HMAC envelope; consumer upsert is idempotent).
    publish_once(&js, &subject, &dir, agent_id, organization_id, &cli_tool, &hmac_secret).await;

    loop {
        tokio::select! {
            changed = shutdown.changed() => {
                if changed.is_ok() && *shutdown.borrow() {
                    tracing::info!("credential watcher shutting down");
                    return Ok(());
                }
            }
            Some(()) = event_rx.recv() => {
                // Debounce: sleep 500ms after first fsnotify event to absorb tmp→rename
                // burst writes, then drain any events that queued during the sleep so a
                // single burst produces exactly one publish. This is the real coalescer
                // — the module used to have a generic Debouncer struct but it was dead
                // code because `js` isn't Clone-able across the FnMut bound.
                tokio::time::sleep(Duration::from_millis(500)).await;
                while event_rx.try_recv().is_ok() {}
                publish_once(&js, &subject, &dir, agent_id, organization_id, &cli_tool, &hmac_secret).await;
            }
        }
    }
}

async fn publish_once(
    js: &JetStreamContext,
    subject: &str,
    dir: &Path,
    agent_id: Uuid,
    organization_id: Uuid,
    cli_tool: &str,
    hmac_secret: &str,
) {
    let outcome = match build_message(dir, agent_id, organization_id, cli_tool).await {
        Ok(outcome) => outcome,
        Err(err) => {
            tracing::warn!(error = %err, "credential sync skipped (build failed)");
            metrics::counter!("credential_sync_publish_errors_total", "reason" => "build_failed").increment(1);
            return;
        }
    };
    let msg = match outcome {
        BuildOutcome::Message(m) => m,
        BuildOutcome::Empty => return,
    };

    let timestamp = chrono::Utc::now().timestamp();
    let payload_json = match serde_json::to_value(&msg) {
        Ok(v) => v,
        Err(err) => {
            metrics::counter!("credential_sync_publish_errors_total", "reason" => "serialize_failed").increment(1);
            tracing::error!(error = %err, "credential sync: serialize failed");
            return;
        }
    };
    let envelope = match SignedEnvelope::sign(hmac_secret.as_bytes(), &agent_id.to_string(), timestamp, &payload_json) {
        Ok(e) => e,
        Err(err) => {
            metrics::counter!("credential_sync_publish_errors_total", "reason" => "sign_failed").increment(1);
            tracing::error!(error = %err, "credential sync: sign failed");
            return;
        }
    };
    let bytes = match serde_json::to_vec(&envelope) {
        Ok(b) => b,
        Err(err) => {
            metrics::counter!(
                "credential_sync_publish_errors_total",
                "reason" => "envelope_encode_failed"
            )
            .increment(1);
            tracing::error!(error = %err, "credential sync: envelope encode failed");
            return;
        }
    };

    match js.publish(subject.to_string(), bytes.into()).await {
        Ok(ack) => {
            if let Err(err) = ack.await {
                metrics::counter!(
                    "credential_sync_publish_errors_total",
                    "reason" => "ack_failed"
                )
                .increment(1);
                tracing::error!(
                    error = %err,
                    subject,
                    file_count = msg.files.len(),
                    "credential sync LOST — user must re-authenticate (no WAL retry on sidecar; NATS unreachable)"
                );
            } else {
                metrics::counter!(
                    "credential_sync_published_total",
                    "cli_tool" => cli_tool.to_string(),
                )
                .increment(1);
                tracing::info!(subject, file_count = msg.files.len(), "credential sync published");
            }
        }
        Err(err) => {
            metrics::counter!(
                "credential_sync_publish_errors_total",
                "reason" => "publish_failed"
            )
            .increment(1);
            tracing::error!(
                error = %err,
                subject,
                file_count = msg.files.len(),
                "credential sync LOST — user must re-authenticate (no WAL retry on sidecar; NATS unreachable)"
            );
        }
    }
}

/// Read all `.json` files in `dir`, enforce size caps, build a
/// `CredentialSyncMessage`. Returns `BuildOutcome::Empty` when the directory
/// contains no readable credential files (valid fresh-container state —
/// caller no-ops without publishing).
pub async fn build_message(dir: &Path, agent_id: Uuid, organization_id: Uuid, cli_tool: &str) -> Result<BuildOutcome> {
    let mut entries = tokio::fs::read_dir(dir).await.with_context(|| format!("read_dir {}", dir.display()))?;
    let mut files = std::collections::BTreeMap::new();
    let mut total: usize = 0;
    while let Some(entry) = entries.next_entry().await? {
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.ends_with(".json") {
            continue;
        }
        if !entry.file_type().await?.is_file() {
            continue;
        }
        if files.len() >= MAX_CREDENTIAL_FILES {
            return Err(anyhow!("too many credential files (> {MAX_CREDENTIAL_FILES})"));
        }
        let contents = tokio::fs::read_to_string(entry.path()).await?;
        if contents.len() > MAX_CREDENTIAL_FILE_BYTES {
            return Err(anyhow!(
                "credential file {name} exceeds {MAX_CREDENTIAL_FILE_BYTES} bytes (got {})",
                contents.len()
            ));
        }
        total = total.saturating_add(contents.len());
        if total > MAX_CREDENTIAL_TOTAL_BYTES {
            return Err(anyhow!("credential file total exceeds {MAX_CREDENTIAL_TOTAL_BYTES} bytes"));
        }
        files.insert(name, contents);
    }
    if files.is_empty() {
        return Ok(BuildOutcome::Empty);
    }
    Ok(BuildOutcome::Message(CredentialSyncMessage {
        agent_id,
        organization_id,
        cli_tool: cli_tool.to_string(),
        files,
    }))
}

fn is_credential_file(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()).is_some_and(|e| e == "json")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn build_message_packages_only_json_files_under_limits() {
        let dir = tempdir().unwrap();
        tokio::fs::write(dir.path().join("auth.json"), r#"{"ok": true}"#).await.unwrap();
        tokio::fs::write(dir.path().join("readme.txt"), "nope").await.unwrap();

        let agent_id = Uuid::new_v4();
        let org_id = Uuid::new_v4();
        let msg_outcome = build_message(dir.path(), agent_id, org_id, "claude").await.unwrap();
        let msg = match msg_outcome {
            BuildOutcome::Message(m) => m,
            BuildOutcome::Empty => panic!("expected a message, got Empty"),
        };

        assert_eq!(msg.agent_id, agent_id);
        assert_eq!(msg.organization_id, org_id);
        assert_eq!(msg.cli_tool, "claude");
        assert_eq!(msg.files.len(), 1, "only .json files are packaged");
        assert_eq!(msg.files.get("auth.json").unwrap(), r#"{"ok": true}"#);
    }

    #[tokio::test]
    async fn build_message_returns_none_when_dir_is_empty() {
        let dir = tempdir().unwrap();
        let outcome = build_message(dir.path(), Uuid::new_v4(), Uuid::new_v4(), "claude").await.unwrap();
        assert!(outcome.is_none_message());
    }

    #[tokio::test]
    async fn build_message_rejects_oversized_single_file() {
        let dir = tempdir().unwrap();
        let huge = "x".repeat(MAX_CREDENTIAL_FILE_BYTES + 1);
        tokio::fs::write(dir.path().join("auth.json"), &huge).await.unwrap();
        let err = build_message(dir.path(), Uuid::new_v4(), Uuid::new_v4(), "claude").await.unwrap_err();
        assert!(err.to_string().contains("exceeds"), "err = {err}");
    }

    #[tokio::test]
    async fn build_message_rejects_too_many_files() {
        let dir = tempdir().unwrap();
        for i in 0..(MAX_CREDENTIAL_FILES + 1) {
            tokio::fs::write(dir.path().join(format!("f{i}.json")), "{}").await.unwrap();
        }
        let err = build_message(dir.path(), Uuid::new_v4(), Uuid::new_v4(), "claude").await.unwrap_err();
        assert!(err.to_string().contains("too many"), "err = {err}");
    }

    #[tokio::test]
    async fn build_message_rejects_total_over_limit() {
        let dir = tempdir().unwrap();
        // 6 files × 50 KiB each = 300 KiB (over the 256 KiB total cap)
        let chunk = "a".repeat(50 * 1024);
        for i in 0..6 {
            tokio::fs::write(dir.path().join(format!("f{i}.json")), &chunk).await.unwrap();
        }
        let err = build_message(dir.path(), Uuid::new_v4(), Uuid::new_v4(), "claude").await.unwrap_err();
        assert!(err.to_string().contains("total"), "err = {err}");
    }

    // Small test helper so the Empty-outcome test reads naturally.
    impl BuildOutcome {
        fn is_none_message(&self) -> bool {
            matches!(self, BuildOutcome::Empty)
        }
    }
}

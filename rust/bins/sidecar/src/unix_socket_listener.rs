//! Unix-domain-socket listener for the CLI relay hook.
//!
//! `hooks/agentforge-relay-hook.cjs` is a thin node script that the wrapped CLI
//! (claude/codex/gemini/opencode) invokes for every hook event. It connects to
//! the shared [`RELAY_SOCKET_PATH`] (`/tmp/agentforge-relay.sock`), writes a
//! **4-byte big-endian length header** followed by the UTF-8 JSON event, then
//! closes. No ack is expected. The sidecar binds that same hardcoded path — the
//! entrypoint and image healthcheck poll it too — so there is no env override to
//! drift out of sync.
//!
//! Historically nothing in the Rust tree bound that socket, so every hook event
//! was silently dropped and the agent console printed
//! `Sidecar relay socket not ready after 5s — events will be lost`. This module
//! restores the listener: it binds the socket, decodes each framed event, and
//! **durably** publishes it through the same HMAC/NATS path native sidecar events
//! use. Durability is **WAL-first**: the event is written to the WAL before the
//! publish attempt and only acknowledged (removed) once the NATS server confirms
//! receipt via `flush()`. An event accepted during a NATS reconnect window thus
//! survives a sidecar restart/OOM — the periodic drain task (see `main.rs`)
//! replays anything still pending when NATS returns.

use std::path::Path;
use std::sync::Arc;

use tokio::io::AsyncReadExt;
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{Semaphore, watch};

use crate::publisher::EventPublisher;
use crate::wal::Wal;

/// Reject frames larger than this (DoS guard). Hook events are small JSON blobs;
/// 10 MiB is generous headroom over the hook's own 64K response truncation.
const MAX_FRAME_SIZE: u32 = 10 * 1024 * 1024;

/// Upper bound on the confirm-handoff `flush()` in the WAL-first relay path.
///
/// `async_nats::Client::flush()` resolves only once the buffered write reaches
/// the server; while NATS is unreachable it blocks **indefinitely** (the flush
/// observer is held until the next successful connection). Without this bound a
/// single hook event accepted during an outage would wedge its connection task
/// forever. On timeout we treat the flush as unconfirmed → the WAL record is
/// KEPT and the periodic drain retries after reconnect. 5s comfortably covers a
/// healthy round-trip while failing fast during an outage.
const FLUSH_CONFIRM_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

/// Maximum number of connection-handler tasks that may run concurrently.
///
/// Applying a bound here prevents unbounded task fan-out when a chatty CLI
/// hammers the relay socket. The accept loop itself applies backpressure —
/// `acquire_owned()` waits for a free slot instead of spawning without limit.
const MAX_CONCURRENT_RELAY_CONNECTIONS: usize = 256;

/// The Unix socket the CLI relay hook (`agentforge-relay-hook.cjs`) connects to.
///
/// This is a single hardcoded source of truth shared with three other places:
/// the hook's own default (`hooks/agentforge-relay-hook.cjs`), the container
/// entrypoint (`docker/scripts/agent-entrypoint.sh`), and the image healthcheck
/// (`docker/Dockerfile.agent-base`). There is deliberately **no** env override:
/// if the sidecar bound a different path the entrypoint/healthcheck would keep
/// polling this one, report "relay socket not ready", and could mark the
/// container unhealthy.
pub const RELAY_SOCKET_PATH: &str = "/tmp/agentforge-relay.sock";

/// Bind the relay Unix socket **owner-only**, closing the TOCTOU window where the
/// socket node would otherwise be briefly group/other-accessible between `bind()`
/// (which honours the process umask — typically 0o022 → a 0o755 node) and the
/// `chmod` to 0o600 (F065).
///
/// We tighten the umask to `0o077` only across the `bind`, so the node is created
/// `0o700` (never world-accessible) and is then refined to the exact `0o600`
/// owner-rw intent. The prior umask is restored immediately.
/// Serializes the process-global `umask` mutation in [`bind_relay_socket_owner_only`]
/// so overlapping callers (notably parallel tests) cannot restore each other's
/// mask. In production the bind happens once, before any task is spawned, so
/// this lock is uncontended.
static UMASK_GUARD: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn bind_relay_socket_owner_only(path: &Path) -> std::io::Result<UnixListener> {
    // Hold the guard across the umask mutation + bind so a concurrent caller
    // cannot interleave and restore the wrong mask.
    let umask_guard = UMASK_GUARD.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    // SAFETY: `umask` is an infallible libc call that returns the previous mask.
    // It is process-global; we hold the tightened mask only across this single
    // `bind` and restore it immediately. Callers must invoke this before spawning
    // concurrent tasks (see `bind_relay_listener`) so no other task creates a file
    // during the window.
    let prev_umask = unsafe { libc::umask(0o077 as libc::mode_t) };
    let bind_result = UnixListener::bind(path);
    unsafe { libc::umask(prev_umask) };
    drop(umask_guard);
    let listener = bind_result?;

    // Refine to the exact owner-only rw intent. Best-effort: with the umask above
    // the node is already 0o700, so a chmod failure leaves it owner-only (not a
    // world-accessible hole) — log and continue rather than fail startup.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Err(err) = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)) {
            tracing::warn!(error = %err, "Failed to chmod relay socket to 0o600 (umask already restricts it to 0o700)");
        }
    }
    Ok(listener)
}

/// Remove any stale socket and bind the relay listener **owner-only**. Call this
/// from the main task **before spawning any other task**, so the brief
/// process-global umask change in [`bind_relay_socket_owner_only`] runs while the
/// process is effectively single-threaded and cannot affect concurrent file
/// creation (F062/F065). The returned listener is then served by [`run`].
pub fn bind_relay_listener(socket_path: &str) -> anyhow::Result<UnixListener> {
    let path = Path::new(socket_path);

    // Remove a stale socket left by a previous (possibly SIGKILLed) sidecar so
    // bind() doesn't fail with EADDRINUSE.
    if path.exists()
        && let Err(err) = std::fs::remove_file(path)
    {
        tracing::warn!(error = %err, socket = %socket_path, "Failed to remove stale relay socket");
    }

    let listener = bind_relay_socket_owner_only(path)?;
    tracing::info!(socket = %socket_path, "Relay socket listener bound");
    Ok(listener)
}

/// Serve framed hook events on a pre-bound relay listener until shutdown.
///
/// Accepts connections on the pre-bound `listener` in a loop; each connection is
/// handled on its own task so a slow or malformed peer cannot stall the listener.
/// On shutdown the socket file at `socket_path` is removed. The socket is bound
/// owner-only by [`bind_relay_listener`] before any task spawns.
pub async fn run(
    listener: UnixListener,
    socket_path: &str,
    publisher: Arc<EventPublisher>,
    wal: Arc<Wal>,
    mut shutdown_rx: watch::Receiver<bool>,
) -> anyhow::Result<()> {
    let path = Path::new(socket_path);

    let semaphore = Arc::new(Semaphore::new(MAX_CONCURRENT_RELAY_CONNECTIONS));

    loop {
        tokio::select! {
            changed = shutdown_rx.changed() => {
                // Sender dropped or shutdown signalled — either way, stop.
                if changed.is_err() || *shutdown_rx.borrow() {
                    break;
                }
            }
            accepted = listener.accept() => {
                match accepted {
                    Ok((stream, _addr)) => {
                        let publisher = publisher.clone();
                        let wal = wal.clone();
                        // Acquire a permit before spawning; the accept loop
                        // blocks here when MAX_CONCURRENT_RELAY_CONNECTIONS tasks
                        // are already running, bounding fan-out.
                        let permit = match semaphore.clone().acquire_owned().await {
                            Ok(permit) => permit,
                            Err(err) => {
                                // The semaphore is never closed in this loop, so this is
                                // unreachable in practice — degrade gracefully instead of
                                // panicking the whole listener task if it ever is.
                                tracing::warn!(error = %err, "Relay connection semaphore closed; skipping connection");
                                continue;
                            }
                        };
                        tokio::spawn(async move {
                            // Permit is held for the lifetime of this task and
                            // released automatically when the task completes.
                            let _permit = permit;
                            if let Err(err) = handle_connection(stream, publisher, wal).await {
                                tracing::warn!(error = %err, "Relay connection handling failed");
                            }
                        });
                    }
                    Err(err) => {
                        tracing::warn!(error = %err, "Relay socket accept failed");
                    }
                }
            }
        }
    }

    // Best-effort cleanup so a restart binds cleanly.
    if let Err(err) = std::fs::remove_file(path) {
        tracing::debug!(error = %err, socket = %socket_path, "Relay socket already removed on shutdown");
    }
    tracing::info!("Relay socket listener stopped");
    Ok(())
}

/// Read one length-prefixed frame from `stream`, decode it, and durably publish.
async fn handle_connection(
    mut stream: UnixStream,
    publisher: Arc<EventPublisher>,
    wal: Arc<Wal>,
) -> anyhow::Result<()> {
    // 4-byte big-endian length header.
    let mut header = [0u8; 4];
    stream.read_exact(&mut header).await?;
    let len = u32::from_be_bytes(header);

    if len > MAX_FRAME_SIZE {
        anyhow::bail!("relay frame too large: {len} bytes (max {MAX_FRAME_SIZE})");
    }

    let mut buf = vec![0u8; len as usize];
    stream.read_exact(&mut buf).await?;

    let value = match decode_frame(&buf) {
        Ok(value) => value,
        Err(err) => {
            // Malformed JSON: log and drop this connection only — the listener
            // survives so a single bad hook payload can't take down event relay.
            tracing::warn!(error = %err, "Discarding malformed relay frame");
            return Ok(());
        }
    };

    let event_type = value.get("type").and_then(|t| t.as_str()).unwrap_or("unknown").to_string();

    durably_publish(&publisher, &wal, &event_type, value).await;
    Ok(())
}

/// Decode a frame body into a JSON value. Pure and exhaustively unit-tested.
fn decode_frame(body: &[u8]) -> Result<serde_json::Value, serde_json::Error> {
    serde_json::from_slice(body)
}

/// Build a WAL record in the exact shape `main.rs`'s startup/periodic replay
/// expects: `{"payload":{"event_type":..,"data":..}}`. Keeping this aligned with
/// the replay reader is load-bearing — a mismatch means the buffered event is
/// silently skipped on replay.
fn wal_record(event_type: &str, data: &serde_json::Value) -> Vec<u8> {
    let record = serde_json::json!({
        "payload": {
            "event_type": event_type,
            "data": data,
        }
    });
    // json!() of owned/borrowed values never fails to serialize.
    serde_json::to_vec(&record).unwrap_or_default()
}

/// What to do with the WAL record after a publish+flush attempt.
#[derive(Debug, PartialEq, Eq)]
enum WalAction {
    /// Server confirmed receipt — remove the buffered record.
    Ack,
    /// Publish or flush failed — keep the record for the periodic drain to retry.
    Keep,
}

/// Pure decision for the WAL-first durability contract, factored out so the
/// keep-vs-ack rule is exhaustively unit-testable without a live NATS server
/// (the real `async-nats` client buffers publishes and returns `Ok` even while
/// disconnected, so the failure branches can't be forced through the I/O path).
///
/// We only acknowledge — i.e. delete the durable copy — when the publish was
/// enqueued **and** the subsequent `flush()` confirmed the server received it.
/// Any failure leaves the record so no relay event is lost across a
/// reconnect + restart.
fn durable_publish_outcome(published_ok: bool, flushed_ok: bool) -> WalAction {
    if published_ok && flushed_ok { WalAction::Ack } else { WalAction::Keep }
}

/// Durably relay one event with a **WAL-first, confirmed-handoff** contract:
///
/// 1. Write the replay-compatible WAL record *first* (before any publish).
/// 2. `publish()` the event (enqueues it in the async-nats client buffer).
/// 3. On publish success, `flush()` to confirm the NATS server accepted it.
/// 4. Only after a confirmed flush, `acknowledge()` (delete) the WAL record.
///
/// On any publish or flush failure the WAL record is left in place; the periodic
/// drain task in `main.rs` retries it once NATS reconnects. This closes the
/// reconnect-window loss: `publish()` returns `Ok` as soon as the message is
/// buffered in the client — *before* the server accepts it — so without the
/// flush+WAL-first ordering an event accepted mid-reconnect would be lost if the
/// sidecar restarted before the buffer drained.
async fn durably_publish(publisher: &EventPublisher, wal: &Wal, event_type: &str, data: serde_json::Value) {
    // 1. Persist before attempting to publish so a crash after enqueue but
    //    before flush still leaves a replayable copy.
    let record = wal_record(event_type, &data);
    let wal_path = match wal.append(&record).await {
        Ok(Some(path)) => path,
        Ok(None) => {
            // WAL is full — the event was already counted as dropped with a
            // warning logged by `append`; do not attempt an unbuffered publish
            // (that would undermine backpressure) and do not treat this as an error.
            return;
        }
        Err(err) => {
            // We couldn't even persist — fall back to a best-effort publish so
            // the event isn't dropped outright, but we can't guarantee delivery.
            tracing::error!(error = %err, event_type, "Failed to write relay event to WAL — attempting unbuffered publish");
            if let Err(pub_err) = publisher.publish(event_type, data).await {
                tracing::error!(error = %pub_err, event_type, "Unbuffered relay publish failed — event lost");
            }
            return;
        }
    };

    // 2-4. Publish, confirm the flush, and acknowledge only on a confirmed
    //       handoff; otherwise keep the record for the periodic drain to retry.
    match attempt_durable_publish(publisher, event_type, data).await {
        WalAction::Ack => {
            tracing::debug!(event_type, "Relay event published and flushed");
            if let Err(err) = wal.acknowledge(&wal_path).await {
                tracing::warn!(error = %err, event_type, "Relay event delivered but WAL ack failed — drain will re-send (harmless without dedup)");
            }
        }
        WalAction::Keep => {
            // Record stays on disk; periodic drain retries on reconnect.
        }
    }
}

/// Publish one event and confirm the handoff, returning whether its WAL record
/// should be [`WalAction::Ack`]'d (deleted) or [`WalAction::Keep`]'d (retried).
///
/// Shared by the live relay path ([`durably_publish`]) and the WAL drain
/// ([`drain_wal`]) so both honour the identical confirmed-handoff rule: a record
/// is acknowledged only when `publish()` enqueued **and** the bounded `flush()`
/// confirmed the server received it. `data` is consumed by the publish.
async fn attempt_durable_publish(publisher: &EventPublisher, event_type: &str, data: serde_json::Value) -> WalAction {
    // Publish enqueues into the async-nats client buffer (returns Ok even while
    // disconnected, which is exactly why the flush confirmation below matters).
    let published_ok = match publisher.publish(event_type, data).await {
        Ok(()) => true,
        Err(err) => {
            tracing::warn!(error = %err, event_type, "Relay publish failed — leaving event in WAL for drain");
            false
        }
    };

    // Flush to confirm the server received the buffered message. Bounded by a
    // timeout because flush() blocks indefinitely while NATS is unreachable — a
    // timeout means "unconfirmed", so the WAL record is kept.
    let flushed_ok = if published_ok {
        match tokio::time::timeout(FLUSH_CONFIRM_TIMEOUT, publisher.flush()).await {
            Ok(Ok(())) => true,
            Ok(Err(err)) => {
                tracing::warn!(error = %err, event_type, "Relay flush failed — leaving event in WAL for drain");
                false
            }
            Err(_elapsed) => {
                tracing::warn!(
                    event_type,
                    timeout_secs = FLUSH_CONFIRM_TIMEOUT.as_secs(),
                    "Relay flush timed out (NATS unreachable) — leaving event in WAL for drain"
                );
                false
            }
        }
    } else {
        false
    };

    durable_publish_outcome(published_ok, flushed_ok)
}

/// Drain the WAL through `publisher` with the **same** WAL-first confirmed-handoff
/// contract as [`durably_publish`]: each buffered event is published, the flush
/// is confirmed, and the record is acknowledged (deleted) only on a confirmed
/// handoff — otherwise it is left for the next drain.
///
/// Shared by the startup replay and the periodic drain in `main.rs`. Previously
/// the drain acknowledged on `publish()` Ok alone, which re-opened the
/// reconnect-window loss the WAL exists to prevent: a publish buffered during a
/// NATS outage was deleted from the WAL and then lost if NATS dropped before the
/// server confirmed (F062).
pub(crate) async fn drain_wal(wal: &Wal, publisher: &EventPublisher) {
    // Skip records younger than the confirm-handoff timeout: the live relay path
    // may still be publishing them, and re-publishing here would duplicate the
    // event (F066). A record the live path failed to confirm ages past this
    // window and is retried on a later drain.
    let entries = match wal.replay_older_than(FLUSH_CONFIRM_TIMEOUT).await {
        Ok(entries) => entries,
        Err(err) => {
            tracing::warn!(error = %err, "Failed to read WAL for drain");
            return;
        }
    };
    for (path, entry) in entries {
        let msg = match serde_json::from_slice::<serde_json::Value>(&entry) {
            Ok(msg) => msg,
            Err(err) => {
                tracing::warn!(error = %err, path = %path.display(), "Failed to deserialize WAL entry, skipping");
                continue;
            }
        };
        let event_type = msg["payload"]["event_type"].as_str().unwrap_or("unknown").to_string();
        let data = msg["payload"]["data"].clone();
        match attempt_durable_publish(publisher, &event_type, data).await {
            WalAction::Ack => {
                if let Err(err) = wal.acknowledge(&path).await {
                    tracing::warn!(error = %err, path = %path.display(), "Failed to acknowledge WAL entry");
                }
            }
            WalAction::Keep => {
                tracing::debug!(path = %path.display(), "WAL entry not confirmed — keeping for next drain");
            }
        }
    }
}

/// Decide whether the periodic WAL-drain task should attempt a drain this tick.
/// Pure so the decision can be unit-tested without a live NATS connection.
///
/// We only drain when NATS is connected **and** the WAL has pending entries.
/// Draining while disconnected would just re-buffer every record; skipping the
/// empty case avoids needless directory scans.
pub fn should_drain(nats_connected: bool, pending_wal_entries: usize) -> bool {
    nats_connected && pending_wal_entries > 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_frame_happy_path() {
        let body = br#"{"type":"pre_tool_use","sessionId":"s1","id":"e1"}"#;
        let value = decode_frame(body).expect("valid JSON decodes");
        assert_eq!(value["type"], "pre_tool_use");
        assert_eq!(value["sessionId"], "s1");
    }

    #[test]
    fn decode_frame_rejects_malformed_json() {
        let body = b"{not valid json";
        assert!(decode_frame(body).is_err());
    }

    #[test]
    fn decode_frame_rejects_truncated_utf8() {
        // 0xFF is never valid UTF-8 / JSON.
        let body = &[0xff, 0xfe, 0xfd];
        assert!(decode_frame(body).is_err());
    }

    #[test]
    fn decode_frame_accepts_empty_object() {
        let value = decode_frame(b"{}").expect("empty object is valid");
        assert!(value.is_object());
        // Missing `type` → caller defaults to "unknown".
        assert!(value.get("type").is_none());
    }

    #[test]
    fn event_type_defaults_to_unknown_when_absent() {
        let value = decode_frame(b"{\"sessionId\":\"s1\"}").unwrap();
        let event_type = value.get("type").and_then(|t| t.as_str()).unwrap_or("unknown");
        assert_eq!(event_type, "unknown");
    }

    #[test]
    fn max_frame_size_is_ten_mebibytes() {
        // Pins the cap; a header over it is rejected by the length check in
        // handle_connection (this constant is the single source of truth).
        assert_eq!(MAX_FRAME_SIZE, 10 * 1024 * 1024);
    }

    #[test]
    fn wal_record_matches_replay_reader_shape() {
        // main.rs replay reads msg["payload"]["event_type"] and
        // msg["payload"]["data"] — this record MUST round-trip through that path.
        let data = serde_json::json!({"tool": "Bash", "sessionId": "s1"});
        let bytes = wal_record("pre_tool_use", &data);
        let parsed: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(parsed["payload"]["event_type"], "pre_tool_use");
        assert_eq!(parsed["payload"]["data"]["tool"], "Bash");
        assert_eq!(parsed["payload"]["data"]["sessionId"], "s1");
    }

    #[test]
    fn should_drain_only_when_connected_and_nonempty() {
        assert!(should_drain(true, 1));
        assert!(should_drain(true, 100));
        assert!(!should_drain(true, 0)); // connected but empty
        assert!(!should_drain(false, 5)); // entries but disconnected
        assert!(!should_drain(false, 0)); // neither
    }

    #[tokio::test]
    async fn relay_socket_is_owner_only_after_bind() {
        use std::os::unix::fs::PermissionsExt;

        let tmp = tempfile::tempdir().unwrap();
        let sock = tmp.path().join("relay.sock");
        // tokio's UnixListener::bind needs a runtime context (provided by tokio::test).
        let _listener = bind_relay_socket_owner_only(&sock).expect("bind owner-only");

        // The socket node must be exactly owner-rw (0o600) — never group/other
        // accessible, even for the instant between bind and chmod (the umask
        // makes the bind-time node 0o700, then chmod refines to 0o600).
        let mode = std::fs::metadata(&sock).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "relay socket must be owner-only, got {mode:o}");
    }

    #[tokio::test]
    async fn umask_makes_bind_owner_only_before_chmod() {
        // The TOCTOU fix itself: with umask 0o077 the socket node is created
        // 0o700 (owner-only) *at bind*, before the refining chmod — so it is
        // never group/other-accessible even for an instant. Without the umask,
        // bind honours the ambient umask (typically 0o022 → a 0o755 node), which
        // is exactly the world-traversable window this closes.
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempfile::tempdir().unwrap();
        let sock = tmp.path().join("raw.sock");

        // Serialize with `bind_relay_socket_owner_only`'s umask mutation so the
        // two umask-changing tests cannot interleave and restore the wrong mask.
        let _umask_guard = UMASK_GUARD.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let prev = unsafe { libc::umask(0o077 as libc::mode_t) };
        let _listener = UnixListener::bind(&sock).unwrap();
        unsafe { libc::umask(prev) };

        let mode = std::fs::metadata(&sock).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o700, "umask 0o077 must make the bind-time node owner-only, got {mode:o}");
    }

    #[test]
    fn relay_socket_path_matches_shared_default() {
        // The const must equal the hardcoded path the hook, entrypoint, and
        // healthcheck all use — there is no env override to reconcile.
        assert_eq!(RELAY_SOCKET_PATH, "/tmp/agentforge-relay.sock");
    }

    #[test]
    fn durable_publish_outcome_acks_only_on_publish_and_flush() {
        // The WAL record is removed *only* when the event was both enqueued and
        // the flush confirmed the server received it. Any failure keeps it so
        // the periodic drain retries — no relay event is lost on reconnect.
        assert_eq!(durable_publish_outcome(true, true), WalAction::Ack);
        assert_eq!(durable_publish_outcome(true, false), WalAction::Keep); // flush failed
        assert_eq!(durable_publish_outcome(false, true), WalAction::Keep); // publish failed (flush n/a)
        assert_eq!(durable_publish_outcome(false, false), WalAction::Keep); // both failed
    }

    /// The WAL-first durability record shape: what `durably_publish()` writes
    /// before every publish must round-trip through `main.rs`'s drain reader.
    /// We exercise the WAL leg directly here (the keep-vs-ack decision is
    /// covered exhaustively by `durable_publish_outcome_acks_only_on_publish_and_flush`).
    #[tokio::test]
    async fn wal_buffering_round_trips_through_drain_shape() {
        let tmp = tempfile::tempdir().unwrap();
        let wal = Wal::new(Some(tmp.path().to_str().unwrap()));

        // What durably_publish() writes before a publish attempt.
        let data = serde_json::json!({"sessionId": "s1", "id": "e1"});
        let path =
            wal.append(&wal_record("session_start", &data)).await.unwrap().expect("append succeeds within capacity");
        assert!(path.exists(), "append returns the path of the written record");

        assert_eq!(wal.pending_count().await.unwrap(), 1);
        let entries = wal.replay().await.unwrap();
        // main.rs drain reads msg["payload"]["event_type"] / ["data"].
        let parsed: serde_json::Value = serde_json::from_slice(&entries[0].1).unwrap();
        assert_eq!(parsed["payload"]["event_type"], "session_start");
        assert_eq!(parsed["payload"]["data"]["sessionId"], "s1");
        assert_eq!(parsed["payload"]["data"]["id"], "e1");
    }

    /// WAL-first contract over a real Unix socket against an **unroutable** NATS
    /// client: `publish()` buffers and returns Ok, but `flush()` can never
    /// confirm delivery, so the WAL record is KEPT (pending stays 1) and the
    /// periodic drain will retry it. This is exactly the reconnect-window
    /// scenario the fix protects: an event accepted while NATS is unreachable is
    /// never silently dropped.
    #[tokio::test]
    async fn handle_connection_keeps_wal_record_when_flush_cannot_confirm() {
        use tokio::io::AsyncWriteExt;

        let tmp = tempfile::tempdir().unwrap();
        let wal = Arc::new(Wal::new(Some(tmp.path().to_str().unwrap())));

        let client = async_nats::ConnectOptions::new()
            .connection_timeout(std::time::Duration::from_millis(50))
            .retry_on_initial_connect()
            .connect("nats://127.0.0.1:1") // unroutable: publish buffers, flush never confirms
            .await
            .expect("lazy connect builds a client without contacting the server");
        let publisher = Arc::new(EventPublisher::new(
            client,
            "00000000-0000-0000-0000-000000000001".to_string(),
            "test-secret",
            Some("claude".to_string()),
            agentforge_core::RuntimeKind::Cli,
        ));

        let sock_path = tmp.path().join("relay.sock");
        let listener = UnixListener::bind(&sock_path).unwrap();
        let pub_clone = publisher.clone();
        let wal_clone = wal.clone();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            handle_connection(stream, pub_clone, wal_clone).await
        });

        let mut client_stream = UnixStream::connect(&sock_path).await.unwrap();
        let event = br#"{"type":"session_start","sessionId":"s1","id":"e1"}"#;
        let header = (event.len() as u32).to_be_bytes();
        client_stream.write_all(&header).await.unwrap();
        client_stream.write_all(event).await.unwrap();
        client_stream.flush().await.unwrap();
        drop(client_stream);

        assert!(server.await.unwrap().is_ok());
        // Flush could not confirm against the unroutable server → record stays.
        assert_eq!(wal.pending_count().await.unwrap(), 1);
    }

    /// F062: the WAL **drain** must honour the same confirmed-handoff contract as
    /// the live relay path. Against an unroutable NATS client `publish()` buffers
    /// and returns Ok but `flush()` never confirms, so a drained record must be
    /// KEPT (pending stays 1) — not deleted on publish-Ok alone, which would lose
    /// the event if NATS dropped before the server confirmed.
    #[tokio::test]
    async fn drain_wal_keeps_record_when_flush_cannot_confirm() {
        let tmp = tempfile::tempdir().unwrap();
        let wal = Wal::new(Some(tmp.path().to_str().unwrap()));

        // Seed one buffered event in the drain-compatible record shape.
        let record = wal_record("session_start", &serde_json::json!({"sessionId": "s1", "id": "e1"}));
        wal.append(&record).await.unwrap();
        assert_eq!(wal.pending_count().await.unwrap(), 1, "seeded one WAL record");

        let client = async_nats::ConnectOptions::new()
            .connection_timeout(std::time::Duration::from_millis(50))
            .retry_on_initial_connect()
            .connect("nats://127.0.0.1:1") // unroutable: publish buffers, flush never confirms
            .await
            .expect("lazy connect builds a client without contacting the server");
        let publisher = EventPublisher::new(
            client,
            "00000000-0000-0000-0000-000000000001".to_string(),
            "test-secret",
            Some("claude".to_string()),
            agentforge_core::RuntimeKind::Cli,
        );

        drain_wal(&wal, &publisher).await;

        assert_eq!(
            wal.pending_count().await.unwrap(),
            1,
            "drain must keep the record when the flush cannot confirm delivery"
        );
    }

    /// End-to-end frame I/O over a real Unix socket: a client writes a framed
    /// event, the listener accepts it, decodes it, and `handle_connection`
    /// completes without panicking. (WAL retention under the WAL-first contract
    /// is asserted by `handle_connection_keeps_wal_record_when_flush_cannot_confirm`.)
    #[tokio::test]
    async fn handle_connection_processes_valid_frame() {
        use tokio::io::AsyncWriteExt;

        let tmp = tempfile::tempdir().unwrap();
        let wal = Arc::new(Wal::new(Some(tmp.path().to_str().unwrap())));

        let client = async_nats::ConnectOptions::new()
            .connection_timeout(std::time::Duration::from_millis(50))
            .retry_on_initial_connect()
            .connect("nats://127.0.0.1:1") // unroutable: publish buffers, no panic
            .await
            .expect("lazy connect builds a client without contacting the server");
        let publisher = Arc::new(EventPublisher::new(
            client,
            "00000000-0000-0000-0000-000000000001".to_string(),
            "test-secret",
            Some("claude".to_string()),
            agentforge_core::RuntimeKind::Cli,
        ));

        let sock_path = tmp.path().join("relay.sock");
        let listener = UnixListener::bind(&sock_path).unwrap();

        let pub_clone = publisher.clone();
        let wal_clone = wal.clone();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            handle_connection(stream, pub_clone, wal_clone).await
        });

        let mut client_stream = UnixStream::connect(&sock_path).await.unwrap();
        let event = br#"{"type":"session_start","sessionId":"s1","id":"e1"}"#;
        let header = (event.len() as u32).to_be_bytes();
        client_stream.write_all(&header).await.unwrap();
        client_stream.write_all(event).await.unwrap();
        client_stream.flush().await.unwrap();
        drop(client_stream);

        // A well-formed frame is processed without error.
        assert!(server.await.unwrap().is_ok());
    }

    /// An oversize length header is rejected without reading the (unsent) body.
    #[tokio::test]
    async fn handle_connection_rejects_oversize_frame() {
        use tokio::io::AsyncWriteExt;

        let tmp = tempfile::tempdir().unwrap();
        let wal = Arc::new(Wal::new(Some(tmp.path().to_str().unwrap())));
        let client = async_nats::ConnectOptions::new()
            .connection_timeout(std::time::Duration::from_millis(50))
            .retry_on_initial_connect()
            .connect("nats://127.0.0.1:1")
            .await
            .unwrap();
        let publisher = Arc::new(EventPublisher::new(
            client,
            "00000000-0000-0000-0000-000000000001".to_string(),
            "test-secret",
            Some("claude".to_string()),
            agentforge_core::RuntimeKind::Cli,
        ));

        let sock_path = tmp.path().join("relay.sock");
        let listener = UnixListener::bind(&sock_path).unwrap();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            // Expect an Err (frame too large) — handle_connection returns it.
            handle_connection(stream, publisher, wal).await
        });

        let mut client_stream = UnixStream::connect(&sock_path).await.unwrap();
        let header = (MAX_FRAME_SIZE + 1).to_be_bytes();
        client_stream.write_all(&header).await.unwrap();
        client_stream.flush().await.unwrap();
        drop(client_stream);

        let result = server.await.unwrap();
        assert!(result.is_err(), "oversize frame must be rejected");
    }

    /// A malformed JSON body is dropped (Ok, not Err) and nothing is buffered —
    /// the listener survives a bad payload.
    #[tokio::test]
    async fn handle_connection_drops_malformed_json_without_buffering() {
        use tokio::io::AsyncWriteExt;

        let tmp = tempfile::tempdir().unwrap();
        let wal = Arc::new(Wal::new(Some(tmp.path().to_str().unwrap())));
        let client = async_nats::ConnectOptions::new()
            .connection_timeout(std::time::Duration::from_millis(50))
            .retry_on_initial_connect()
            .connect("nats://127.0.0.1:1")
            .await
            .unwrap();
        let publisher = Arc::new(EventPublisher::new(
            client,
            "00000000-0000-0000-0000-000000000001".to_string(),
            "test-secret",
            Some("claude".to_string()),
            agentforge_core::RuntimeKind::Cli,
        ));

        let sock_path = tmp.path().join("relay.sock");
        let listener = UnixListener::bind(&sock_path).unwrap();
        let wal_check = wal.clone();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            handle_connection(stream, publisher, wal).await
        });

        let mut client_stream = UnixStream::connect(&sock_path).await.unwrap();
        let body = b"{not json";
        let header = (body.len() as u32).to_be_bytes();
        client_stream.write_all(&header).await.unwrap();
        client_stream.write_all(body).await.unwrap();
        client_stream.flush().await.unwrap();
        drop(client_stream);

        // Malformed JSON is handled gracefully (Ok), not buffered.
        assert!(server.await.unwrap().is_ok());
        assert_eq!(wal_check.pending_count().await.unwrap(), 0);
    }
}

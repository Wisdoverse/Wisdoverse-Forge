//! Unix-domain-socket listener for the CLI relay hook.
//!
//! `hooks/agentforge-relay-hook.cjs` is a thin node script that the wrapped CLI
//! (claude/codex/gemini/opencode) invokes for every hook event. It connects to a
//! Unix socket (`AGENTFORGE_RELAY_SOCKET`, default `/tmp/agentforge-relay.sock`),
//! writes a **4-byte big-endian length header** followed by the UTF-8 JSON event,
//! then closes. No ack is expected.
//!
//! Historically nothing in the Rust tree bound that socket, so every hook event
//! was silently dropped and the agent console printed
//! `Sidecar relay socket not ready after 5s — events will be lost`. This module
//! restores the listener: it binds the socket, decodes each framed event, and
//! **durably** publishes it through the same HMAC/NATS path native sidecar events
//! use. On publish failure the event is appended to the WAL in a replay-compatible
//! shape so the periodic drain task (see `main.rs`) flushes it when NATS returns.

use std::path::Path;
use std::sync::Arc;

use tokio::io::AsyncReadExt;
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::watch;

use crate::publisher::EventPublisher;
use crate::wal::Wal;

/// Reject frames larger than this (DoS guard). Hook events are small JSON blobs;
/// 10 MiB is generous headroom over the hook's own 64K response truncation.
const MAX_FRAME_SIZE: u32 = 10 * 1024 * 1024;

/// Bind the relay socket and serve framed hook events until shutdown.
///
/// Removes any stale socket file, binds a fresh [`UnixListener`], tightens the
/// socket to owner-only (`0o600` — hook and sidecar run as the same agent user),
/// then accepts connections in a loop. Each connection is handled on its own task
/// so a slow or malformed peer cannot stall the listener. On shutdown the socket
/// file is removed.
pub async fn run(
    socket_path: &str,
    publisher: Arc<EventPublisher>,
    wal: Arc<Wal>,
    mut shutdown_rx: watch::Receiver<bool>,
) -> anyhow::Result<()> {
    let path = Path::new(socket_path);

    // Remove a stale socket left by a previous (possibly SIGKILLed) sidecar so
    // bind() doesn't fail with EADDRINUSE.
    if path.exists()
        && let Err(err) = std::fs::remove_file(path)
    {
        tracing::warn!(error = %err, socket = %socket_path, "Failed to remove stale relay socket");
    }

    let listener = UnixListener::bind(path)?;

    // Owner-only: the hook and sidecar are the same user inside the container.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Err(err) = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)) {
            tracing::warn!(error = %err, socket = %socket_path, "Failed to chmod relay socket to 0o600");
        }
    }

    tracing::info!(socket = %socket_path, "Relay socket listener bound");

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
                        tokio::spawn(async move {
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

/// Publish the event; on failure, append it to the WAL so it isn't lost during a
/// NATS outage. The periodic drain task in `main.rs` replays the WAL when NATS
/// reconnects.
async fn durably_publish(publisher: &EventPublisher, wal: &Wal, event_type: &str, data: serde_json::Value) {
    match publisher.publish(event_type, data.clone()).await {
        Ok(()) => {
            tracing::debug!(event_type, "Relay event published");
        }
        Err(err) => {
            tracing::warn!(error = %err, event_type, "Relay publish failed — buffering to WAL");
            let record = wal_record(event_type, &data);
            if let Err(wal_err) = wal.append(&record).await {
                tracing::error!(error = %wal_err, event_type, "Failed to buffer relay event to WAL — event lost");
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
        assert_eq!(MAX_FRAME_SIZE, 10 * 1024 * 1024);
        // A header just over the cap is rejected by the length check in
        // handle_connection (the constant is the single source of truth).
        assert!(MAX_FRAME_SIZE + 1 > MAX_FRAME_SIZE);
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

    /// The append-on-failure durability contract: when a publish fails, the
    /// event is buffered to the WAL in the exact record shape `main.rs`'s drain
    /// reads back. We exercise the WAL leg directly (a real `async-nats` client
    /// buffers publishes and returns Ok even when disconnected, so it cannot
    /// deterministically force the failure branch in a unit test).
    #[tokio::test]
    async fn wal_buffering_round_trips_through_drain_shape() {
        let tmp = tempfile::tempdir().unwrap();
        let wal = Wal::new(Some(tmp.path().to_str().unwrap()));

        // What durably_publish() writes on a publish failure.
        let data = serde_json::json!({"sessionId": "s1", "id": "e1"});
        wal.append(&wal_record("session_start", &data)).await.unwrap();

        assert_eq!(wal.pending_count().await.unwrap(), 1);
        let entries = wal.replay().await.unwrap();
        // main.rs drain reads msg["payload"]["event_type"] / ["data"].
        let parsed: serde_json::Value = serde_json::from_slice(&entries[0].1).unwrap();
        assert_eq!(parsed["payload"]["event_type"], "session_start");
        assert_eq!(parsed["payload"]["data"]["sessionId"], "s1");
        assert_eq!(parsed["payload"]["data"]["id"], "e1");
    }

    /// End-to-end frame I/O over a real Unix socket: a client writes a framed
    /// event, the listener accepts it, decodes it, and `handle_connection`
    /// completes without panicking. A real `async-nats` client buffers the
    /// publish (returns Ok) so nothing is appended to the WAL here — the WAL-
    /// append branch is covered by `wal_buffering_round_trips_through_drain_shape`.
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

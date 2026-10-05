//! Owner-only local Windows relay, authenticated before the hook sends content.
use std::io;
use std::os::windows::io::AsRawHandle;
use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::windows::named_pipe::{NamedPipeServer, ServerOptions};
use tokio::sync::{Semaphore, watch};
use tokio::task::JoinSet;

use crate::publisher::EventPublisher;
use crate::wal::Wal;
use crate::windows_security::PrivateSecurityDescriptor;

const MAX_CONNECTIONS: usize = 32;
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(2);

pub fn pipe_name(agent_id: uuid::Uuid) -> String {
    format!(r"\\.\pipe\agentforge-relay-{agent_id}")
}

fn create_instance(name: &str, first: bool) -> io::Result<NamedPipeServer> {
    let descriptor = PrivateSecurityDescriptor::new()?;
    let mut attributes = descriptor.attributes();
    // SAFETY: attributes and its descriptor remain alive through pipe creation.
    // No inherited handles or default/world-readable DACL are used.
    unsafe {
        ServerOptions::new()
            .first_pipe_instance(first)
            .reject_remote_clients(true)
            .max_instances(MAX_CONNECTIONS + 1)
            .create_with_security_attributes_raw(name, std::ptr::from_mut(&mut attributes).cast())
    }
}

pub fn bind(agent_id: uuid::Uuid) -> io::Result<NamedPipeServer> {
    let server = create_instance(&pipe_name(agent_id), true)?;
    crate::windows_security::verify_private_handle(server.as_raw_handle())?;
    Ok(server)
}

async fn handle(mut stream: NamedPipeServer, publisher: Arc<EventPublisher>, wal: Arc<Wal>) -> anyhow::Result<()> {
    let mut nonce = [0u8; 32];
    tokio::time::timeout(HANDSHAKE_TIMEOUT, async {
        stream.read_exact(&mut nonce).await?;
        stream.write_all(&publisher.relay_server_tag(&nonce)).await?;
        stream.flush().await
    })
    .await??;
    crate::unix_socket_listener::handle_connection(stream, publisher, wal).await
}

pub async fn run(
    mut listener: NamedPipeServer,
    agent_id: uuid::Uuid,
    publisher: Arc<EventPublisher>,
    wal: Arc<Wal>,
    mut shutdown: watch::Receiver<bool>,
) -> anyhow::Result<()> {
    let name = pipe_name(agent_id);
    let permits = Arc::new(Semaphore::new(MAX_CONNECTIONS));
    let mut tasks = JoinSet::new();
    loop {
        tokio::select! {
            changed = shutdown.changed() => {
                if changed.is_err() || *shutdown.borrow() { break; }
            }
            joined = tasks.join_next(), if !tasks.is_empty() => {
                if let Some(Err(err)) = joined {
                    tracing::warn!(error = %err, "Relay connection task failed");
                }
            }
            connected = listener.connect() => {
                connected?;
                let permit = tokio::select! {
                    changed = shutdown.changed() => {
                        if changed.is_err() || *shutdown.borrow() { break; }
                        continue;
                    }
                    permit = permits.clone().acquire_owned() => permit?,
                };
                // Rearm BEFORE dropping the connected handle. The pipe name is
                // continuously held, so another account cannot claim it between
                // connections. All instances use the same private ACL.
                let next = create_instance(&name, false)?;
                let stream = std::mem::replace(&mut listener, next);
                let publisher = publisher.clone();
                let wal = wal.clone();
                tasks.spawn(async move {
                    let _permit = permit;
                    if let Err(err) = handle(stream, publisher, wal).await {
                        tracing::warn!(error = %err, "Relay connection rejected or interrupted");
                    }
                });
            }
        }
    }
    drop(listener);
    // Frame reads and NATS flushes are bounded. Preserve admitted events before
    // exiting, but do not let a silent local client stall shutdown indefinitely.
    if tokio::time::timeout(Duration::from_secs(10), async { while tasks.join_next().await.is_some() {} })
        .await
        .is_err()
    {
        tasks.abort_all();
        while tasks.join_next().await.is_some() {}
    }
    Ok(())
}

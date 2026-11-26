//! In-process fake Redis for tests (CI has no Redis service).
//!
//! A RESP2 server on a local TCP port that answers the handshake the `redis`
//! crate sends (pipelined `CLIENT SETINFO`, optional `AUTH`/`SELECT`: all `+OK`)
//! plus `PING`, `SET` (with `EX`/`GET` options), `SETEX`, `GET`, `GETDEL`,
//! `DEL` and `EXISTS`. Expiry is not modelled. A test can stop it (closes the
//! listener and every connection), start it again on the same port, and switch
//! it into the failure [`Mode`]s the reconnection tests need.
//!
//! Compiled only for `agentforge-infra`'s own tests or when the `test-support` feature is
//! enabled explicitly; nothing in the workspace enables it outside tests.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::task::{JoinHandle, JoinSet};

/// How the fake answers commands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// Answer every command.
    Normal,
    /// Read commands but never answer (a hung server or a network blackhole).
    Blackhole,
    /// Answer every command, each after this delay.
    Delay(Duration),
    /// Accept new connections and close them at once (connection drops during
    /// a reconnection attempt).
    DropOnAccept,
    /// Answer writes with a `READONLY` error (a read-only replica).
    ReadOnly,
    /// Reject `AUTH` (a disabled Redis user); everything else is answered.
    DenyAuth,
}

#[derive(Default)]
struct State {
    data: Mutex<HashMap<Vec<u8>, Vec<u8>>>,
    mode: Mutex<Option<Mode>>,
    commands: Mutex<Vec<String>>,
    /// Connections are numbered as accepted; those below this number never answer again.
    next_connection: Mutex<usize>,
    silenced_below: Mutex<usize>,
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// A fake Redis server bound to `127.0.0.1`.
pub struct FakeRedis {
    addr: SocketAddr,
    state: Arc<State>,
    accept: Option<JoinHandle<()>>,
}

impl FakeRedis {
    /// Start on a free port.
    pub async fn start() -> Self {
        Self::start_at(SocketAddr::from(([127, 0, 0, 1], 0))).await
    }

    /// Start on `addr` (use [`FakeRedis::unused_addr`] for a port nothing listens on yet).
    pub async fn start_at(addr: SocketAddr) -> Self {
        let mut fake = Self { addr, state: Arc::default(), accept: None };
        fake.listen().await;
        fake
    }

    /// A local address with no listener (connections to it are refused).
    pub fn unused_addr() -> SocketAddr {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind a free port");
        listener.local_addr().expect("local addr")
    }

    /// `redis://` URL for [`crate::RedisClient::connect`].
    pub fn url(&self) -> String {
        Self::url_for(self.addr)
    }

    /// `redis://` URL for an address.
    pub fn url_for(addr: SocketAddr) -> String {
        format!("redis://{addr}/")
    }

    /// `redis://` URL with a password, so the client sends `AUTH` during its handshake.
    pub fn url_with_password(&self, password: &str) -> String {
        format!("redis://:{password}@{}/", self.addr)
    }

    /// Keep every connection open now open, but never answer on it again (a silent firewall,
    /// or a peer that acknowledges TCP but stops answering). New connections are answered.
    pub fn silence_open_connections(&self) {
        *lock(&self.state.silenced_below) = *lock(&self.state.next_connection);
    }

    pub fn set_mode(&self, mode: Mode) {
        *lock(&self.state.mode) = Some(mode);
    }

    /// How many commands named `name` (upper case) the server has received.
    pub fn received(&self, name: &str) -> usize {
        lock(&self.state.commands).iter().filter(|cmd| *cmd == name).count()
    }

    /// Close the listener and every open connection. Data and mode are kept.
    pub async fn stop(&mut self) {
        if let Some(accept) = self.accept.take() {
            accept.abort();
            let _ = accept.await;
        }
    }

    /// Listen again on the same port.
    pub async fn restart(&mut self) {
        self.stop().await;
        self.listen().await;
    }

    async fn listen(&mut self) {
        let listener = TcpListener::bind(self.addr).await.expect("bind fake redis port");
        self.addr = listener.local_addr().expect("local addr");
        let state = self.state.clone();
        self.accept = Some(tokio::spawn(async move {
            // Dropping this set when the accept task is aborted aborts every connection.
            let mut connections = JoinSet::new();
            while let Ok((stream, _)) = listener.accept().await {
                connections.spawn(serve(stream, state.clone()));
            }
        }));
    }
}

impl Drop for FakeRedis {
    fn drop(&mut self) {
        if let Some(accept) = &self.accept {
            accept.abort();
        }
    }
}

async fn serve(stream: TcpStream, state: Arc<State>) {
    let id = {
        let mut next = lock(&state.next_connection);
        *next += 1;
        *next - 1
    };
    let mode = || lock(&state.mode).unwrap_or(Mode::Normal);
    if mode() == Mode::DropOnAccept {
        return;
    }
    let (read, mut write) = stream.into_split();
    let mut read = BufReader::new(read);
    while let Some(args) = read_command(&mut read).await {
        let name = String::from_utf8_lossy(&args[0]).to_ascii_uppercase();
        lock(&state.commands).push(name.clone());
        if id < *lock(&state.silenced_below) {
            continue;
        }
        let reply = match mode() {
            Mode::Blackhole => continue,
            Mode::Delay(delay) => {
                tokio::time::sleep(delay).await;
                reply(&state, &name, &args, false)
            }
            Mode::ReadOnly => reply(&state, &name, &args, true),
            Mode::DenyAuth if name == "AUTH" => {
                b"-WRONGPASS invalid username-password pair or user is disabled.\r\n".to_vec()
            }
            Mode::Normal | Mode::DropOnAccept | Mode::DenyAuth => reply(&state, &name, &args, false),
        };
        if write.write_all(&reply).await.is_err() {
            return;
        }
    }
}

/// Read one RESP array of bulk strings; `None` on EOF or malformed input.
async fn read_command(read: &mut BufReader<tokio::net::tcp::OwnedReadHalf>) -> Option<Vec<Vec<u8>>> {
    async fn header(read: &mut BufReader<tokio::net::tcp::OwnedReadHalf>, prefix: char) -> Option<usize> {
        let mut line = String::new();
        if read.read_line(&mut line).await.ok()? == 0 {
            return None;
        }
        line.trim_end().strip_prefix(prefix)?.parse().ok()
    }
    let count = header(read, '*').await?;
    let mut args = Vec::with_capacity(count);
    for _ in 0..count {
        let len = header(read, '$').await?;
        let mut buf = vec![0; len + 2];
        read.read_exact(&mut buf).await.ok()?;
        buf.truncate(len);
        args.push(buf);
    }
    (!args.is_empty()).then_some(args)
}

fn reply(state: &State, name: &str, args: &[Vec<u8>], read_only: bool) -> Vec<u8> {
    fn bulk(value: Option<Vec<u8>>) -> Vec<u8> {
        match value {
            Some(value) => [format!("${}\r\n", value.len()).into_bytes(), value, b"\r\n".to_vec()].concat(),
            None => b"$-1\r\n".to_vec(),
        }
    }
    let arg = |i: usize| args.get(i).cloned().unwrap_or_default();
    let mut data = lock(&state.data);
    match name {
        "PING" => b"+PONG\r\n".to_vec(),
        "SET" | "SETEX" if read_only => b"-READONLY You can't write against a read only replica.\r\n".to_vec(),
        "SETEX" => {
            data.insert(arg(1), arg(3));
            b"+OK\r\n".to_vec()
        }
        "SET" => {
            let prior = data.insert(arg(1), arg(2));
            let wants_prior = args.iter().skip(3).any(|option| option.eq_ignore_ascii_case(b"GET"));
            if wants_prior { bulk(prior) } else { b"+OK\r\n".to_vec() }
        }
        "GET" => bulk(data.get(&arg(1)).cloned()),
        "GETDEL" => bulk(data.remove(&arg(1))),
        "DEL" | "EXISTS" => {
            let keys = &args[1..];
            let hits = if name == "DEL" {
                keys.iter().filter(|key| data.remove(*key).is_some()).count()
            } else {
                keys.iter().filter(|key| data.contains_key(*key)).count()
            };
            format!(":{hits}\r\n").into_bytes()
        }
        // Handshake (`CLIENT SETINFO`, `AUTH`, `SELECT`) and anything else.
        _ => b"+OK\r\n".to_vec(),
    }
}

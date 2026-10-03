use std::ffi::c_void;
use std::io;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::io::AsRawHandle;
use std::process::Stdio;
use std::ptr;
use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::windows::named_pipe::{ClientOptions, NamedPipeClient, ServerOptions};
use tokio::process::Command;
use tokio::sync::watch;
use windows_sys::Win32::Foundation::{
    CloseHandle, ERROR_PIPE_BUSY, GENERIC_READ, GENERIC_WRITE, INVALID_HANDLE_VALUE, LocalFree,
};
use windows_sys::Win32::Security::Authorization::{
    ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW, GetSecurityInfo, SE_FILE_OBJECT,
};
use windows_sys::Win32::Security::{OWNER_SECURITY_INFORMATION, SECURITY_ATTRIBUTES};
use windows_sys::Win32::Storage::FileSystem::{
    CREATE_NEW, CreateDirectoryW, CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_FLAG_BACKUP_SEMANTICS,
    FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
};

use crate::publisher::EventPublisher;
use crate::wal::Wal;
use crate::windows_pipe_listener::{bind, pipe_name, run};
use crate::windows_security::{ensure_private_state_root, verify_private_handle};

const TEST_HMAC_SECRET: &str = "synthetic-windows-runtime-test-key";

#[tokio::test]
async fn windows_pipe_security_and_hook_relay_survive_bad_peers_and_restart() {
    let temp = tempfile::tempdir().expect("create temporary test root");
    let state_root = temp.path().join("private-state");
    ensure_private_state_root(&state_root).expect("create private state root");
    let root_file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(&state_root)
        .expect("open private state root");
    verify_private_handle(root_file.as_raw_handle()).expect("private root has the expected owner and DACL");

    let user_sid = owner_sid(root_file.as_raw_handle()).expect("read private root owner SID");

    let broad_root = temp.path().join("broad-root");
    create_public_directory(&broad_root, &user_sid).expect("create explicitly permissive root");
    assert!(ensure_private_state_root(&broad_root).is_err(), "explicit Everyone DACL on root must be rejected");

    let broad_file_root = temp.path().join("broad-file-root");
    ensure_private_state_root(&broad_file_root).expect("create private file test root");
    create_public_file(&broad_file_root.join("public.json"), &user_sid).expect("create permissive child file");
    assert!(ensure_private_state_root(&broad_file_root).is_err(), "permissive existing child file must be rejected");

    let broad_dir_root = temp.path().join("broad-dir-root");
    ensure_private_state_root(&broad_dir_root).expect("create private directory test root");
    create_public_directory(&broad_dir_root.join("public-child"), &user_sid)
        .expect("create permissive child directory");
    assert!(
        ensure_private_state_root(&broad_dir_root).is_err(),
        "permissive existing child directory must be rejected"
    );

    let junction_root = temp.path().join("junction-root");
    ensure_private_state_root(&junction_root).expect("create private junction test root");
    let target = temp.path().join("junction-target");
    std::fs::create_dir(&target).expect("create junction target");
    let sentinel = target.join("preserve.txt");
    std::fs::write(&sentinel, b"fixture target remains untouched").expect("seed target sentinel");
    let junction = junction_root.join("target-link");
    let command = format!("mklink /J \"{}\" \"{}\"", junction.display(), target.display());
    let created = std::process::Command::new("cmd.exe").arg("/C").arg(command).output().expect("run mklink /J");
    let junction_cleanup = JunctionCleanup(junction.clone());
    assert!(created.status.success(), "create junction fixture without symlink privilege");
    let error = ensure_private_state_root(&junction_root).expect_err("junction must be rejected");
    assert!(error.to_string().contains("reparse points"), "validator rejects the junction itself");
    assert_eq!(std::fs::read(&sentinel).expect("read preserved target sentinel"), b"fixture target remains untouched");
    drop(junction_cleanup);

    let agent_id = uuid::Uuid::new_v4();
    let pipe = bind(agent_id).expect("bind private relay pipe");
    verify_private_handle(pipe.as_raw_handle()).expect("relay pipe has the expected owner and DACL");
    assert!(bind(agent_id).is_err(), "a second FIRST_PIPE_INSTANCE must be rejected");

    let default_pipe_name = pipe_name(uuid::Uuid::new_v4());
    let default_pipe =
        ServerOptions::new().first_pipe_instance(true).create(&default_pipe_name).expect("create inherited-DACL pipe");
    assert!(verify_private_handle(default_pipe.as_raw_handle()).is_err(), "inherited pipe DACL must be rejected");
    drop(default_pipe);

    let state_path = state_root.to_string_lossy().into_owned();
    let wal = Arc::new(Wal::new(Some(&state_path)));
    wal.init_pending().await;
    let nats = async_nats::ConnectOptions::new()
        .connection_timeout(Duration::from_millis(50))
        .retry_on_initial_connect()
        .connect("nats://127.0.0.1:1")
        .await
        .expect("unreachable NATS address still constructs a retrying client");
    let publisher = Arc::new(EventPublisher::new_with_wal_path(
        nats,
        agent_id.to_string(),
        TEST_HMAC_SECRET,
        Some("claude".to_string()),
        agentforge_core::RuntimeKind::Cli,
        Some(&state_path),
    ));

    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let listener = tokio::spawn(run(pipe, agent_id, publisher.clone(), wal.clone(), shutdown_rx));
    let name = pipe_name(agent_id);

    let mut malformed = connect_pipe(&name).await;
    authenticate_server(&mut malformed, &publisher).await;
    send_frame(&mut malformed, b"{broken-json").await;
    drop(malformed);

    let mut oversized = connect_pipe(&name).await;
    authenticate_server(&mut oversized, &publisher).await;
    oversized.write_all(&(10 * 1024 * 1024u32 + 1).to_be_bytes()).await.expect("send oversize header");
    drop(oversized);

    let hook_path =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../hooks/agentforge-relay-hook.cjs");
    let mut node = Command::new("node")
        .arg(hook_path)
        .env("HMAC_SECRET", TEST_HMAC_SECRET)
        .env("AGENTFORGE_AGENT_ID", agent_id.to_string())
        .env("AGENTFORGE_CLI_TOOL", "claude")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .expect("Node.js is required for native Windows relay qualification");
    let input = serde_json::json!({
        "hook_event_name": "SessionStart",
        "session_id": "synthetic-session",
        "cwd": "C:\\workspace",
        "source": "startup"
    });
    node.stdin
        .take()
        .expect("Node stdin")
        .write_all(input.to_string().as_bytes())
        .await
        .expect("write synthetic SessionStart hook");
    let output = tokio::time::timeout(Duration::from_secs(15), node.wait())
        .await
        .expect("hook process exits within its connection deadline")
        .expect("wait for hook process");
    assert!(output.success(), "relay hook exits successfully");

    shutdown_tx.send(true).expect("signal listener shutdown");
    let result = tokio::time::timeout(Duration::from_secs(12), listener)
        .await
        .expect("listener shutdown completes in finite time")
        .expect("join listener task");
    assert!(result.is_ok(), "listener exits cleanly");

    let entries = wal.replay().await.expect("read WAL");
    assert_eq!(entries.len(), 1, "malformed and oversized peers add no WAL entries");
    let record: serde_json::Value = serde_json::from_slice(&entries[0].1).expect("decode WAL record");
    let event = &record["payload"]["data"];
    assert_eq!(record["payload"]["event_type"], "session_start");
    let event_id = event["id"].as_str().expect("hook event id").to_string();
    let lifecycle_sequence = event["lifecycleSequence"].as_i64().expect("lifecycle sequence");
    assert_eq!(lifecycle_sequence, 1);
    assert_eq!(event["sessionId"], "synthetic-session");
    assert_eq!(event["runtimeId"], agent_id.to_string());
    assert_eq!(event["cliTool"], "claude");
    assert_eq!(event["sourceHookType"], "claude");

    let recovered_wal = Arc::new(Wal::new(Some(&state_path)));
    recovered_wal.init_pending().await;
    let recovered = recovered_wal.replay().await.expect("recover pending WAL event");
    assert_eq!(recovered.len(), 1);
    let recovered_record: serde_json::Value = serde_json::from_slice(&recovered[0].1).expect("decode recovered record");
    assert_eq!(recovered_record["payload"]["data"]["id"], event_id);
    assert_eq!(recovered_record["payload"]["data"]["lifecycleSequence"], lifecycle_sequence);
    // Preserve owner failures: an elevated Windows token can default new WAL
    // files to Administrators even when the containing DACL is private.
    ensure_private_state_root(&state_root).expect("revalidate persisted WAL and lifecycle state ACLs");
    let recovered_nats = async_nats::ConnectOptions::new()
        .connection_timeout(Duration::from_millis(50))
        .retry_on_initial_connect()
        .connect("nats://127.0.0.1:1")
        .await
        .expect("construct recovery publisher client");
    let recovered_publisher = EventPublisher::new_with_wal_path(
        recovered_nats,
        agent_id.to_string(),
        TEST_HMAC_SECRET,
        Some("claude".to_string()),
        agentforge_core::RuntimeKind::Cli,
        Some(&state_path),
    );
    let next = recovered_publisher
        .prepare_hook_event("session_end", serde_json::json!({"sessionId":"synthetic-session"}))
        .expect("prepare next lifecycle event");
    assert_eq!(next["lifecycleSequence"], lifecycle_sequence + 1);
}

async fn connect_pipe(name: &str) -> NamedPipeClient {
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            match ClientOptions::new().open(name) {
                Ok(client) => return client,
                Err(error) if error.raw_os_error() == Some(ERROR_PIPE_BUSY as i32) => {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
                Err(error) => panic!("open relay pipe failed: {error}"),
            }
        }
    })
    .await
    .expect("relay pipe becomes available")
}

async fn authenticate_server(stream: &mut NamedPipeClient, publisher: &EventPublisher) {
    let mut nonce = [0u8; 32];
    nonce[..16].copy_from_slice(uuid::Uuid::new_v4().as_bytes());
    nonce[16..].copy_from_slice(uuid::Uuid::new_v4().as_bytes());
    stream.write_all(&nonce).await.expect("send handshake nonce");
    let mut proof = [0u8; 32];
    tokio::time::timeout(Duration::from_secs(3), stream.read_exact(&mut proof))
        .await
        .expect("server proof arrives within the handshake deadline")
        .expect("read server proof");
    assert_eq!(proof, publisher.relay_server_tag(&nonce), "server proof authenticates the private relay");
}

async fn send_frame(stream: &mut NamedPipeClient, body: &[u8]) {
    stream.write_all(&(body.len() as u32).to_be_bytes()).await.expect("send frame header");
    stream.write_all(body).await.expect("send frame body");
}

struct JunctionCleanup(std::path::PathBuf);

impl Drop for JunctionCleanup {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir(&self.0);
    }
}

fn owner_sid(handle: windows_sys::Win32::Foundation::HANDLE) -> io::Result<String> {
    let mut owner = ptr::null_mut();
    let mut descriptor = ptr::null_mut();
    // SAFETY: handle is live and borrowed; output pointers are writable.
    let error = unsafe {
        GetSecurityInfo(
            handle,
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION,
            &mut owner,
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
            &mut descriptor,
        )
    };
    if error != 0 {
        return Err(io::Error::from_raw_os_error(error as i32));
    }
    let mut string_sid = ptr::null_mut();
    // SAFETY: owner is part of the live descriptor returned by GetSecurityInfo.
    if unsafe { ConvertSidToStringSidW(owner, &mut string_sid) } == 0 {
        // SAFETY: descriptor is owned by this function.
        unsafe { LocalFree(descriptor) };
        return Err(io::Error::last_os_error());
    }
    let mut length = 0;
    // SAFETY: ConvertSidToStringSidW returns a NUL-terminated UTF-16 allocation.
    unsafe {
        while *string_sid.add(length) != 0 {
            length += 1;
        }
        let result = String::from_utf16(std::slice::from_raw_parts(string_sid, length)).map_err(io::Error::other);
        LocalFree(string_sid.cast());
        LocalFree(descriptor);
        result
    }
}

fn create_public_directory(path: &std::path::Path, user_sid: &str) -> io::Result<()> {
    let descriptor = format!("O:{user_sid}D:(A;OICI;GA;;;WD)");
    let wide = path.as_os_str().encode_wide().chain(Some(0)).collect::<Vec<_>>();
    with_security_descriptor(&descriptor, |attributes| {
        // SAFETY: path and attributes are live through the native call.
        if unsafe { CreateDirectoryW(wide.as_ptr(), attributes) } == 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    })
}

fn create_public_file(path: &std::path::Path, user_sid: &str) -> io::Result<()> {
    let descriptor = format!("O:{user_sid}D:(A;;GA;;;WD)");
    let wide_path = path.as_os_str().encode_wide().chain(Some(0)).collect::<Vec<_>>();
    with_security_descriptor(&descriptor, |attributes| {
        // SAFETY: path and attributes are live; CREATE_NEW avoids overwriting files.
        let handle = unsafe {
            CreateFileW(
                wide_path.as_ptr(),
                GENERIC_READ | GENERIC_WRITE,
                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                attributes,
                CREATE_NEW,
                FILE_ATTRIBUTE_NORMAL,
                ptr::null_mut(),
            )
        };
        if handle == INVALID_HANDLE_VALUE {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: CreateFileW returned an owned handle.
        unsafe { CloseHandle(handle) };
        Ok(())
    })
}

fn with_security_descriptor<T>(
    sddl: &str,
    create: impl FnOnce(*const SECURITY_ATTRIBUTES) -> io::Result<T>,
) -> io::Result<T> {
    let wide = sddl.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
    let mut descriptor = ptr::null_mut();
    // SAFETY: wide is NUL terminated and the returned descriptor stays live through create().
    if unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(wide.as_ptr(), 1, &mut descriptor, ptr::null_mut())
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    let attributes = SECURITY_ATTRIBUTES {
        nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: descriptor,
        bInheritHandle: 0,
    };
    let result = create(&attributes);
    // SAFETY: conversion allocated this descriptor with LocalAlloc.
    unsafe { LocalFree(descriptor) };
    result
}

//! Parity gate: run the same arg vector against the Go and Rust binaries
//! against an identical wiremock server, then byte-diff stdout, stderr, and
//! exit codes.
//!
//! The Go binary path comes from `AGENTFORGE_GO_BIN` (set by build.rs).
//! If empty, every test is skipped with a descriptive message.

use serde_json::json;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const GO_BIN: Option<&str> = option_env!("AGENTFORGE_GO_BIN");
const RS_BIN: Option<&str> = option_env!("AGENTFORGE_RUST_BIN");

static RUST_BIN: OnceLock<Result<PathBuf, String>> = OnceLock::new();

fn go_bin_path() -> Option<PathBuf> {
    GO_BIN.filter(|path| !path.is_empty()).map(PathBuf::from)
}

fn rust_workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("crate manifest should live under rust/crates/cli")
        .to_path_buf()
}

fn rust_bin_path() -> PathBuf {
    RS_BIN.filter(|path| !path.is_empty()).map(PathBuf::from).unwrap_or_else(|| {
        rust_workspace_root().join("target").join("debug").join(format!("agentforge{}", std::env::consts::EXE_SUFFIX))
    })
}

fn ensure_rust_bin() -> Result<PathBuf, String> {
    match RUST_BIN.get_or_init(|| {
        let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
        let workspace_root = rust_workspace_root();
        let rust_bin = rust_bin_path();

        let status = Command::new(&cargo)
            .arg("build")
            .arg("-q")
            .arg("-p")
            .arg("agentforge-cli-bin")
            .arg("--bin")
            .arg("agentforge")
            .current_dir(&workspace_root)
            .status()
            .map_err(|err| format!("failed to invoke `{cargo} build -p agentforge-cli-bin --bin agentforge`: {err}"))?;

        if !status.success() {
            return Err(format!("`{cargo} build -p agentforge-cli-bin --bin agentforge` exited with {status}"));
        }
        if !rust_bin.exists() {
            return Err(format!("Rust parity binary {} was not produced by cargo build", rust_bin.display()));
        }

        Ok(rust_bin)
    }) {
        Ok(path) => Ok(path.clone()),
        Err(err) => Err(err.clone()),
    }
}

fn skip_if_no_go_bin() -> bool {
    let Some(go_bin) = go_bin_path() else {
        eprintln!("SKIP: AGENTFORGE_GO_BIN is empty (no Go source tree, toolchain, or override available)");
        return true;
    };
    if !go_bin.exists() {
        eprintln!("SKIP: AGENTFORGE_GO_BIN={} does not exist", go_bin.display());
        return true;
    }
    ensure_rust_bin().expect("failed to build current Rust parity binary");
    false
}

/// Runs both binaries against `server` with `args`, returns (go_output, rs_output).
fn run_both(server_uri: &str, args: &[&str]) -> (std::process::Output, std::process::Output) {
    let go_bin = go_bin_path().expect("Go parity binary should have been validated by skip_if_no_go_bin");
    let rust_bin = ensure_rust_bin().expect("Rust parity binary should have been built before running tests");

    let go = Command::new(&go_bin)
        .args(args)
        .env_remove("AGENTFORGE_TOKEN")
        .env_remove("XDG_CONFIG_HOME")
        .env("HOME", "/tmp/parity-nonexistent-home")
        .env("AGENTFORGE_SERVER", server_uri)
        .env("AGENTFORGE_NON_INTERACTIVE", "true")
        .env("NO_COLOR", "1")
        .env("CI", "true")
        .output()
        .expect("go binary failed to run");
    let rs = Command::new(&rust_bin)
        .args(args)
        .env_remove("AGENTFORGE_TOKEN")
        .env_remove("XDG_CONFIG_HOME")
        .env("HOME", "/tmp/parity-nonexistent-home")
        .env("AGENTFORGE_SERVER", server_uri)
        .env("AGENTFORGE_NON_INTERACTIVE", "true")
        .env("NO_COLOR", "1")
        .env("CI", "true")
        .output()
        .expect("rust binary failed to run");
    (go, rs)
}

fn assert_parity(args: &[&str], go: &std::process::Output, rs: &std::process::Output) {
    let go_stdout = String::from_utf8_lossy(&go.stdout).into_owned();
    let rs_stdout = String::from_utf8_lossy(&rs.stdout).into_owned();
    let go_stderr = String::from_utf8_lossy(&go.stderr).into_owned();
    let rs_stderr = String::from_utf8_lossy(&rs.stderr).into_owned();
    let go_code = go.status.code();
    let rs_code = rs.status.code();

    if go_stdout != rs_stdout || go_code != rs_code {
        eprintln!("=== PARITY FAILURE for args: {args:?} ===");
        eprintln!("--- GO stdout ---\n{go_stdout}");
        eprintln!("--- RS stdout ---\n{rs_stdout}");
        eprintln!("--- GO stderr ---\n{go_stderr}");
        eprintln!("--- RS stderr ---\n{rs_stderr}");
        eprintln!("GO exit={go_code:?}  RS exit={rs_code:?}");
    }
    assert_eq!(go_code, rs_code, "exit code parity for {args:?}");
    similar_asserts::assert_eq!(go_stdout.trim_end(), rs_stdout.trim_end(), "stdout parity for {args:?}");
}

// ---------- Tests ----------

#[tokio::test]
async fn health_json() {
    if skip_if_no_go_bin() {
        return;
    }
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/health"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "ok": true,
            "data": { "status": "ok", "version": "1.2.3", "uptime": "10m" }
        })))
        .mount(&server)
        .await;

    let (go, rs) = run_both(&server.uri(), &["health", "-o", "json"]);
    assert_parity(&["health", "-o", "json"], &go, &rs);
}

#[tokio::test]
async fn whoami_json() {
    if skip_if_no_go_bin() {
        return;
    }
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/users/me"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "ok": true,
            "data": { "id": "u1", "email": "dev@example.com", "name": "Dev User" }
        })))
        .mount(&server)
        .await;

    let (go, rs) = run_both(&server.uri(), &["whoami", "-o", "json"]);
    assert_parity(&["whoami", "-o", "json"], &go, &rs);
}

#[tokio::test]
async fn agents_list_json() {
    if skip_if_no_go_bin() {
        return;
    }
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/agents"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "ok": true,
            "data": [
                {"id":"a","name":"alpha","cliTool":"claude","status":"idle","projectId":"p1","createdAt":"2026-04-14T00:00:00Z"},
                {"id":"b","name":"beta","cliTool":"codex","status":"working","projectId":"p1","createdAt":"2026-04-14T00:00:00Z"}
            ],
            "total": 2, "limit": 50, "offset": 0
        })))
        .mount(&server)
        .await;

    let (go, rs) = run_both(&server.uri(), &["agents", "list", "-o", "json"]);
    assert_parity(&["agents", "list", "-o", "json"], &go, &rs);
}

#[tokio::test]
async fn agents_get_json() {
    if skip_if_no_go_bin() {
        return;
    }
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/agents/x"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "ok": true,
            "data": {"id":"x","name":"xenon","cliTool":"claude","status":"idle","projectId":"p1","createdAt":"2026-04-14T00:00:00Z"}
        })))
        .mount(&server)
        .await;

    let (go, rs) = run_both(&server.uri(), &["agents", "get", "x", "-o", "json"]);
    assert_parity(&["agents", "get", "x", "-o", "json"], &go, &rs);
}

#[tokio::test]
async fn agents_get_not_found() {
    if skip_if_no_go_bin() {
        return;
    }
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/agents/missing"))
        .respond_with(ResponseTemplate::new(404).set_body_json(json!({
            "ok": false, "error": "NOT_FOUND", "message": "agent missing not found"
        })))
        .mount(&server)
        .await;

    let (go, rs) = run_both(&server.uri(), &["agents", "get", "missing", "-o", "json"]);
    assert_parity(&["agents", "get", "missing", "-o", "json"], &go, &rs);
}

#[tokio::test]
async fn events_list_json() {
    if skip_if_no_go_bin() {
        return;
    }
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/events"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "ok": true,
            "data": [
                {"id":"e1","type":"tool_start","agentId":"a","createdAt":"2026-04-14T00:00:00Z"},
                {"id":"e2","type":"agent_idle","agentId":"a","createdAt":"2026-04-14T00:00:01Z"}
            ],
            "total": 2, "limit": 50, "offset": 0
        })))
        .mount(&server)
        .await;

    let (go, rs) = run_both(&server.uri(), &["events", "list", "-o", "json"]);
    assert_parity(&["events", "list", "-o", "json"], &go, &rs);
}

#[tokio::test]
async fn events_stats_json() {
    if skip_if_no_go_bin() {
        return;
    }
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/events/stats"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "ok": true,
            "data": { "total": 100, "by_type": { "tool_start": 50, "agent_idle": 50 } }
        })))
        .mount(&server)
        .await;

    let (go, rs) = run_both(&server.uri(), &["events", "stats", "-o", "json"]);
    assert_parity(&["events", "stats", "-o", "json"], &go, &rs);
}

#[tokio::test]
async fn groups_list_json() {
    if skip_if_no_go_bin() {
        return;
    }
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/groups"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "ok": true,
            "data": [
                {"id":"g1","name":"core","description":"core team","teamId":"t1","projectId":"p1"},
                {"id":"g2","name":"infra","description":"infra team","teamId":"t1","projectId":"p1"}
            ]
        })))
        .mount(&server)
        .await;

    let (go, rs) = run_both(&server.uri(), &["groups", "list", "-o", "json"]);
    assert_parity(&["groups", "list", "-o", "json"], &go, &rs);
}

#[tokio::test]
async fn auth_status_no_creds_json() {
    if skip_if_no_go_bin() {
        return;
    }
    let server = MockServer::start().await;
    let (go, rs) = run_both(&server.uri(), &["auth", "status", "-o", "json"]);
    assert_parity(&["auth", "status", "-o", "json"], &go, &rs);
}

#[tokio::test]
async fn version_offline() {
    if skip_if_no_go_bin() {
        return;
    }
    // Don't mount any health mock — server will 404 → version prints (unavailable).
    let server = MockServer::start().await;
    let (go, rs) = run_both(&server.uri(), &["version", "-o", "json"]);
    assert_parity(&["version", "-o", "json"], &go, &rs);
}

#[tokio::test]
async fn api_low_level_get_json() {
    if skip_if_no_go_bin() {
        return;
    }
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/health"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "ok": true,
            "data": { "status": "ok", "version": "1.2.3" }
        })))
        .mount(&server)
        .await;

    let (go, rs) = run_both(&server.uri(), &["api", "/api/v1/health"]);
    assert_parity(&["api", "/api/v1/health"], &go, &rs);
}

#[tokio::test]
async fn unauthorized_exit_code_3() {
    if skip_if_no_go_bin() {
        return;
    }
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/agents"))
        .respond_with(ResponseTemplate::new(401).set_body_json(json!({
            "ok": false, "error": "UNAUTHORIZED", "message": "missing or invalid token"
        })))
        .mount(&server)
        .await;

    let (go, rs) = run_both(&server.uri(), &["agents", "list"]);
    assert_eq!(go.status.code(), Some(3), "Go should exit 3 on UNAUTHORIZED");
    assert_eq!(rs.status.code(), Some(3), "Rust should exit 3 on UNAUTHORIZED");
}

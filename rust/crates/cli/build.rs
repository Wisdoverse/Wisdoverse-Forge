//! Build script: prepares both the Go and Rust CLI binaries for parity testing.
//!
//! Go binary resolution order:
//!   1. `AGENTFORGE_GO_BIN` env var — explicit path
//!   2. build a fresh parity binary from the nearest ancestor `cli/` source tree
//!   3. nearest ancestor `cli/agentforge` pre-built binary (only when sources are unavailable)
//!   4. Fallback: empty string (parity tests will gracefully skip)
//!
//! Rust binary resolution order:
//!   1. `AGENTFORGE_RUST_BIN` env var — explicit path
//!   2. `<rust-workspace>/target/debug/agentforge` (parity tests build it on demand)
//!   3. `<rust-workspace>/target/release/agentforge`
//!   4. Fallback: empty string (parity tests will gracefully skip)
//!
//! Build metadata resolution order:
//!   1. `AGENTFORGE_CLI_VERSION` / `AGENTFORGE_CLI_COMMIT` / `AGENTFORGE_CLI_DATE`
//!   2. Git-derived values from the current checkout
//!   3. Fallbacks: `dev`, `none`, `unknown`
//!
//! All paths and build metadata are exported via `cargo:rustc-env`.

use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-env-changed=AGENTFORGE_GO_BIN");
    println!("cargo:rerun-if-env-changed=AGENTFORGE_RUST_BIN");
    println!("cargo:rerun-if-env-changed=AGENTFORGE_CLI_VERSION");
    println!("cargo:rerun-if-env-changed=AGENTFORGE_CLI_COMMIT");
    println!("cargo:rerun-if-env-changed=AGENTFORGE_CLI_DATE");

    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap_or_default());
    let rust_workspace_root = find_rust_workspace_root(&manifest_dir);
    let target_dir = resolve_target_dir(rust_workspace_root.as_deref());
    let build_version = resolve_build_version(&manifest_dir);
    let build_commit = resolve_build_commit(&manifest_dir);
    let build_date = resolve_build_date(&manifest_dir);

    // --- Go binary ---
    let go_bin = std::env::var("AGENTFORGE_GO_BIN")
        .ok()
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            if let Some(cli_dir) = find_go_cli_dir(&manifest_dir) {
                register_go_reruns(&cli_dir);
                build_go_parity_bin(&cli_dir, target_dir.as_deref(), &build_version, &build_commit, &build_date)
            } else {
                find_prebuilt_go_bin(&manifest_dir)
            }
        })
        .map(|p| p.display().to_string())
        .unwrap_or_default();

    println!("cargo:rustc-env=AGENTFORGE_GO_BIN={go_bin}");

    // --- Rust binary ---
    let rust_bin = std::env::var("AGENTFORGE_RUST_BIN")
        .ok()
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            let target_dir = target_dir.as_ref()?;
            let debug_bin = target_dir.join("debug").join(binary_name("agentforge"));
            if debug_bin.exists() {
                return Some(debug_bin);
            }
            let release_bin = target_dir.join("release").join(binary_name("agentforge"));
            if release_bin.exists() {
                return Some(release_bin);
            }
            Some(debug_bin)
        })
        .map(|p| p.display().to_string())
        .unwrap_or_default();

    println!("cargo:rustc-env=AGENTFORGE_RUST_BIN={rust_bin}");
    println!("cargo:rustc-env=AGENTFORGE_CLI_VERSION={build_version}");
    println!("cargo:rustc-env=AGENTFORGE_CLI_COMMIT={build_commit}");
    println!("cargo:rustc-env=AGENTFORGE_CLI_DATE={build_date}");
    println!("cargo:rerun-if-changed=build.rs");
}

fn find_rust_workspace_root(manifest_dir: &Path) -> Option<PathBuf> {
    for path in manifest_dir.ancestors() {
        let cargo_toml = path.join("Cargo.toml");
        if cargo_toml.exists()
            && std::fs::read_to_string(&cargo_toml).map(|contents| contents.contains("[workspace]")).unwrap_or(false)
        {
            return Some(path.to_path_buf());
        }
    }
    None
}

fn resolve_target_dir(rust_workspace_root: Option<&Path>) -> Option<PathBuf> {
    std::env::var("CARGO_TARGET_DIR")
        .ok()
        .map(PathBuf::from)
        .or_else(|| rust_workspace_root.map(|root| root.join("target")))
}

fn binary_name(name: &str) -> String {
    format!("{name}{}", std::env::consts::EXE_SUFFIX)
}

fn find_go_cli_dir(manifest_dir: &Path) -> Option<PathBuf> {
    for path in manifest_dir.ancestors() {
        let candidate = path.join("cli");
        if candidate.join("go.mod").exists() && candidate.join("main.go").exists() {
            return Some(candidate);
        }
    }
    None
}

fn find_prebuilt_go_bin(root: &Path) -> Option<PathBuf> {
    for path in root.ancestors() {
        let candidate = path.join("cli").join(binary_name("agentforge"));
        if candidate.exists() {
            return Some(candidate);
        }
    }
    None
}

fn register_go_reruns(cli_dir: &Path) {
    for path in ["go.mod", "go.sum", "main.go", "cmd", "internal"] {
        let candidate = cli_dir.join(path);
        if candidate.exists() {
            println!("cargo:rerun-if-changed={}", candidate.display());
        }
    }
}

fn build_go_parity_bin(
    cli_dir: &Path,
    target_dir: Option<&Path>,
    version: &str,
    commit: &str,
    date: &str,
) -> Option<PathBuf> {
    let target_dir = target_dir?;
    let output_dir = target_dir.join("parity").join("go");
    if let Err(err) = std::fs::create_dir_all(&output_dir) {
        println!("cargo:warning=failed to create Go parity output dir {}: {err}", output_dir.display());
        return None;
    }

    let output = output_dir.join(binary_name("agentforge"));
    let ldflags = format!("-X main.version={version} -X main.commit={commit} -X main.date={date}");
    let Some(go_exe) = find_go_executable() else {
        println!("cargo:warning=go executable not found; parity tests will skip unless AGENTFORGE_GO_BIN is set");
        return None;
    };

    let status = match Command::new(go_exe)
        .arg("build")
        .arg("-o")
        .arg(&output)
        .arg("-ldflags")
        .arg(&ldflags)
        .arg(".")
        .current_dir(cli_dir)
        .status()
    {
        Ok(status) => status,
        Err(err) => {
            println!("cargo:warning=failed to start Go build in {}: {err}", cli_dir.display());
            return None;
        }
    };

    if !status.success() {
        println!("cargo:warning=Go parity build in {} exited with status {}", cli_dir.display(), status);
        return None;
    }

    Some(output)
}

fn find_go_executable() -> Option<PathBuf> {
    if let Some(go) = env_override("GO") {
        let candidate = PathBuf::from(go);
        if candidate.exists() {
            return Some(candidate);
        }
    }

    if let Some(goroot) = std::env::var_os("GOROOT") {
        let candidate = PathBuf::from(goroot).join("bin").join(binary_name("go"));
        if candidate.exists() {
            return Some(candidate);
        }
    }

    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(binary_name("go"));
        if candidate.exists() {
            return Some(candidate);
        }
    }

    if let Some(home) = std::env::var_os("HOME") {
        let installs_dir = PathBuf::from(home).join(".local").join("share").join("mise").join("installs").join("go");
        if let Ok(entries) = std::fs::read_dir(installs_dir) {
            let mut candidates: Vec<PathBuf> = entries
                .filter_map(Result::ok)
                .map(|entry| entry.path().join("bin").join(binary_name("go")))
                .filter(|candidate| candidate.exists())
                .collect();
            candidates.sort();
            if let Some(candidate) = candidates.pop() {
                return Some(candidate);
            }
        }
    }
    None
}

fn env_override(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|value| !value.trim().is_empty())
}

fn git_output(manifest_dir: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git").args(args).current_dir(manifest_dir).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let value = String::from_utf8(output.stdout).ok()?;
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(trimmed.to_string())
}

fn resolve_build_version(manifest_dir: &Path) -> String {
    env_override("AGENTFORGE_CLI_VERSION")
        .or_else(|| git_output(manifest_dir, &["describe", "--tags", "--always", "--dirty"]))
        .unwrap_or_else(|| "dev".to_string())
}

fn resolve_build_commit(manifest_dir: &Path) -> String {
    env_override("AGENTFORGE_CLI_COMMIT")
        .or_else(|| git_output(manifest_dir, &["rev-parse", "--short=12", "HEAD"]))
        .unwrap_or_else(|| "none".to_string())
}

fn resolve_build_date(manifest_dir: &Path) -> String {
    env_override("AGENTFORGE_CLI_DATE")
        .or_else(|| git_output(manifest_dir, &["log", "-1", "--date=iso-strict", "--format=%cd"]))
        .unwrap_or_else(|| "unknown".to_string())
}

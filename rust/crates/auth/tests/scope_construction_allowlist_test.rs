use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use uuid::Uuid;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().parent().unwrap().parent().unwrap().to_path_buf()
}

fn make_fake_repo() -> PathBuf {
    let root = std::env::temp_dir().join(format!("agentforge-scope-guard-{}", Uuid::now_v7()));
    fs::create_dir_all(root.join("rust/crates/auth/src")).expect("create fake auth src");
    root
}

fn write_allowlist(root: &Path, body: &str) {
    fs::write(root.join("rust/crates/auth/src/scope_construction_allowlist.txt"), body).expect("write allowlist");
}

fn run_guard(root: &Path) -> std::process::Output {
    Command::new("sh")
        .arg(repo_root().join("scripts/check-scope-construction.sh"))
        .env("SCOPE_CONSTRUCTION_REPO_ROOT", root)
        .output()
        .expect("run scope construction guard")
}

#[test]
fn guard_passes_current_repository() {
    let output = run_guard(&repo_root());
    assert!(output.status.success(), "guard failed: {}", String::from_utf8_lossy(&output.stderr));
}

#[test]
fn guard_rejects_direct_constructor_outside_allowlist() {
    let root = make_fake_repo();
    write_allowlist(&root, "");
    let bad_path = root.join("rust/crates/api/src/bad.rs");
    fs::create_dir_all(bad_path.parent().unwrap()).expect("create fake api src");
    let constructor = ["TenantScope", "::new(org_id, user_id);"].concat();
    fs::write(&bad_path, constructor).expect("write bad fixture");

    let output = run_guard(&root);

    assert!(!output.status.success(), "guard unexpectedly passed");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("rust/crates/api/src/bad.rs"), "stderr missing path: {stderr}");
    assert!(stderr.contains(&["TenantScope", "::new"].concat()), "stderr missing constructor: {stderr}");
}

#[test]
fn guard_allows_documented_constructor_helper() {
    let root = make_fake_repo();
    write_allowlist(&root, "rust/crates/api/src/test_support.rs # central helper\n");
    let helper_path = root.join("rust/crates/api/src/test_support.rs");
    fs::create_dir_all(helper_path.parent().unwrap()).expect("create fake helper src");
    let constructor = ["TenantScope", "::new(org_id, user_id);"].concat();
    fs::write(&helper_path, constructor).expect("write helper fixture");

    let output = run_guard(&root);

    assert!(output.status.success(), "guard failed: {}", String::from_utf8_lossy(&output.stderr));
}

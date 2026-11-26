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

/// Built at runtime so this file never spells a constructor path itself.
const TS: &str = "TenantScope";

fn write_fixture(root: &Path, rel: &str, source: &str) {
    let path = root.join(rel);
    fs::create_dir_all(path.parent().unwrap()).expect("create fixture dir");
    fs::write(path, source).expect("write fixture");
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

/// F003: a forged scope via `TenantScope::with_axes` outside the allowlist must
/// be flagged — locking the new detection so the blind spot cannot return.
#[test]
fn guard_rejects_with_axes_outside_allowlist() {
    let root = make_fake_repo();
    write_allowlist(&root, "");
    let bad_path = root.join("rust/crates/api/src/forge.rs");
    fs::create_dir_all(bad_path.parent().unwrap()).expect("create fake api src");
    let constructor = ["TenantScope", "::with_axes(org, user, None, None, None);"].concat();
    fs::write(&bad_path, constructor).expect("write with_axes fixture");

    let output = run_guard(&root);

    assert!(!output.status.success(), "guard unexpectedly passed on with_axes");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("rust/crates/api/src/forge.rs"), "stderr missing path: {stderr}");
    assert!(stderr.contains(&["TenantScope", "::with_axes"].concat()), "stderr missing constructor: {stderr}");
}

/// #1324: background work uses the named constructor, which is never flagged.
#[test]
fn guard_allows_background_constructor_outside_allowlist() {
    let root = make_fake_repo();
    write_allowlist(&root, "");
    write_fixture(
        &root,
        "rust/crates/api/src/services/worker.rs",
        &format!(
            "fn sweep(scope: &{TS}) -> impl Fn(&{TS}) + Send {{\n    {TS}::for_background(org, UserId::SYSTEM, None)\n}}\nfn into() -> impl Into<{TS}> {{ x }}\nimpl Sweep for Worker {{\n    fn run(&self) {{ for s in scopes {{ s }} }}\n}}"
        ),
    );

    let output = run_guard(&root);

    assert!(output.status.success(), "guard failed: {}", String::from_utf8_lossy(&output.stderr));
}

/// #1324: cheap evasions of the constructor rule are rejected, each on its own.
#[test]
fn guard_rejects_constructor_evasions() {
    let cases = [
        ("alias_import", format!("use agentforge_core::{TS} as Ts;")),
        ("alias_in_group", format!("use agentforge_core::{{OrgId, {TS} as Scope}};")),
        ("re_export_alias", format!("pub use agentforge_core::{TS} as Scope;")),
        ("type_alias", format!("type Scope = agentforge_core::{TS};")),
        ("spaced_path", format!("fn f() {{ let s = {TS} :: new(o, u); }}")),
        (
            "line_break_before_path",
            format!("fn f() {{\n    let s = {TS}\n        ::with_axes(o, u, None, None, None);\n}}"),
        ),
        ("line_break_after_path", format!("fn f() {{\n    let s = agentforge_core::{TS}::\n        new(o, u);\n}}")),
        ("function_pointer", format!("fn f() {{ ids.map({TS}::new) }}")),
        ("qualified_self_type", format!("fn f() {{ <{TS}>::new(o, u) }}")),
        ("trait_impl_self_new", format!("impl MakeScope for {TS} {{\n    fn make() -> Self {{ Self::new(o, u) }}\n}}")),
        (
            "from_impl",
            format!("impl From<Claims> for agentforge_core::{TS} {{\n    fn from(c: Claims) -> Self {{ todo!() }}\n}}"),
        ),
        ("unchecked_scoped_read", "fn f() { ScopedRead::unchecked_from_parts(o, u) }".to_string()),
        // Multi-line headers: the scanner matches whole statements, not lines.
        ("type_alias_split_lines", format!("type Scope =\n    agentforge_core::{TS};\nfn f() {{ Scope::new(o, u) }}")),
        (
            "trait_impl_split_lines",
            format!("impl MakeScope\n    for {TS}\n{{\n    fn make() -> Self {{ Self::new(o, u) }}\n}}"),
        ),
        (
            "attributed_trait_impl_split_lines",
            format!(
                "#[async_trait]\nimpl MakeScope\nfor agentforge_core::{TS} {{\n    async fn make() -> Self {{ Self::new(o, u) }}\n}}"
            ),
        ),
        ("inherent_impl_split_lines", format!("impl\n{TS} {{\n    fn make() -> Self {{ Self::new(o, u) }}\n}}")),
        ("alias_import_split_lines", format!("use agentforge_core::{{\n    OrgId,\n    {TS}\n        as Scope,\n}};")),
    ];

    for (name, source) in cases {
        let root = make_fake_repo();
        write_allowlist(&root, "");
        let rel = format!("rust/crates/api/src/{name}.rs");
        write_fixture(&root, &rel, &source);

        let output = run_guard(&root);

        assert!(!output.status.success(), "{name}: guard unexpectedly passed on:\n{source}");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains(&format!("{rel}:")), "{name}: stderr missing path: {stderr}");
        assert!(stderr.contains("for_background"), "{name}: stderr missing the background constructor hint: {stderr}");
    }
}

/// #1324: the allowlist is per function, so a new public wrapper in an allowed
/// file is flagged while the allowed function itself passes.
#[test]
fn guard_allows_only_the_listed_function_in_an_allowed_file() {
    let root = make_fake_repo();
    write_allowlist(&root, "rust/crates/auth/src/middleware.rs::from_request_parts # request scope\n");
    let allowed = format!(
        "impl<S> FromRequestParts<S> for AuthUser {{\n    async fn from_request_parts(parts: &mut Parts) -> Result<Self, AppError> {{\n        let scope = {TS}::with_axes(\n            org, user, None, None, None,\n        );\n        Ok(user)\n    }}\n}}\n"
    );
    write_fixture(&root, "rust/crates/auth/src/middleware.rs", &allowed);
    assert!(run_guard(&root).status.success(), "allowed function must pass");

    let wrapper = format!(
        "{allowed}pub fn scope_for(org: OrgId, user: UserId) -> {TS} {{\n    {TS}::with_axes(org, user, None, None, None)\n}}\n"
    );
    write_fixture(&root, "rust/crates/auth/src/middleware.rs", &wrapper);

    let output = run_guard(&root);

    assert!(!output.status.success(), "wrapper in an allowed file must be flagged");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("rust/crates/auth/src/middleware.rs:10: [fn scope_for]"), "stderr: {stderr}");
    assert!(!stderr.contains("[fn from_request_parts]"), "allowed function flagged: {stderr}");
}

/// #1324: the real request-path constructors are seen by the scanner, so the
/// guard cannot silently rot into matching nothing.
#[test]
fn guard_sees_real_request_constructors_when_not_allowlisted() {
    let root = repo_root();
    let allowlist = std::env::temp_dir().join(format!("agentforge-scope-allowlist-{}.txt", Uuid::now_v7()));
    fs::write(&allowlist, "rust/crates/core/src/tenant.rs\nrust/crates/api/src/test_support.rs\n")
        .expect("write narrowed allowlist");

    let output = Command::new("sh")
        .arg(root.join("scripts/check-scope-construction.sh"))
        .env("SCOPE_CONSTRUCTION_REPO_ROOT", &root)
        .env("SCOPE_CONSTRUCTION_ALLOWLIST", &allowlist)
        .output()
        .expect("run scope construction guard");

    assert!(!output.status.success(), "guard unexpectedly passed without the request-path entries");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("rust/crates/auth/src/middleware.rs:"), "stderr missing middleware: {stderr}");
    assert!(stderr.contains("rust/crates/api/src/gateway/ws.rs:"), "stderr missing ws gateway: {stderr}");
}

use std::fs;
use std::path::{Path, PathBuf};

#[test]
fn route_handlers_do_not_reintroduce_ddd_boundary_leaks() {
    let routes_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/routes");
    let mut violations = Vec::new();

    for route in route_files(&routes_dir) {
        let source = fs::read_to_string(&route).expect("read route source");

        for (line_no, line) in production_lines(&source) {
            let trimmed = line.trim();
            if is_allowed_empty_json_default(trimmed) {
                continue;
            }

            if contains_json_macro(trimmed) {
                violations.push(format!(
                    "{}:{} uses json! in production route code; move response construction to domain/service",
                    route.display(),
                    line_no + 1
                ));
            }

            if contains_raw_sql(trimmed) {
                violations.push(format!(
                    "{}:{} uses raw SQL in production route code; move query orchestration to repository/service",
                    route.display(),
                    line_no + 1
                ));
            }

            if contains_repository_namespace_import(trimmed) {
                violations.push(format!(
                    "{}:{} imports repositories in production route code; route handlers should depend on service factories",
                    route.display(),
                    line_no + 1
                ));
            }

            if contains_repository_constructor(trimmed) {
                violations.push(format!(
                    "{}:{} constructs repositories in production route code; move repository wiring to service",
                    route.display(),
                    line_no + 1
                ));
            }

            if contains_runtime_state_wiring(trimmed) {
                violations.push(format!(
                    "{}:{} wires runtime AppState dependencies in production route code; move service construction to AppState factories",
                    route.display(),
                    line_no + 1
                ));
            }

            if contains_route_service_constructor(trimmed) {
                violations.push(format!(
                    "{}:{} constructs runtime-aware services in production route code; use AppState service factories",
                    route.display(),
                    line_no + 1
                ));
            }

            if let Some(name) = route_local_projection_name(trimmed) {
                violations.push(format!(
                    "{}:{} defines route-local projection `{name}`; move response/projection types to domain",
                    route.display(),
                    line_no + 1
                ));
            }

            if contains_runtime_policy_config(trimmed) {
                violations.push(format!(
                    "{}:{} reads runtime policy config in production route code; move runtime wiring to service",
                    route.display(),
                    line_no + 1
                ));
            }

            if contains_identity_repository_wiring(trimmed) {
                violations.push(format!(
                    "{}:{} constructs identity/credential repositories in production route code; move repository wiring to service",
                    route.display(),
                    line_no + 1
                ));
            }

            if contains_agent_orchestration_repository_wiring(trimmed) {
                violations.push(format!(
                    "{}:{} constructs agent/orchestration repositories in production route code; move aggregate wiring to service",
                    route.display(),
                    line_no + 1
                ));
            }

            if contains_runtime_service_factory_wiring(trimmed) {
                violations.push(format!(
                    "{}:{} constructs runtime-aware services in production route code; move runtime factories to service",
                    route.display(),
                    line_no + 1
                ));
            }

            if contains_route_error_policy(trimmed) {
                violations.push(format!(
                    "{}:{} owns ErrorKind policy in production route code; move error contracts to domain/service helpers",
                    route.display(),
                    line_no + 1
                ));
            }
        }
    }

    assert!(violations.is_empty(), "route DDD boundary violations:\n{}", violations.join("\n"));
}

#[test]
fn services_do_not_reintroduce_persistence_or_payload_boundary_leaks() {
    let services_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/services");
    let mut violations = Vec::new();

    for service in rust_files_recursive(&services_dir) {
        let source = fs::read_to_string(&service).expect("read service source");
        for (line_no, line) in production_lines(&source) {
            if contains_raw_sql(line.trim()) {
                violations.push(format!(
                    "{}:{} uses raw SQL in production service code; move tenant-scoped queries to repositories",
                    service.display(),
                    line_no + 1
                ));
            }

            if contains_json_macro(line.trim()) {
                violations.push(format!(
                    "{}:{} uses json! in production service code; move protocol/payload construction to domain",
                    service.display(),
                    line_no + 1
                ));
            }

            if contains_service_serde_adapter(line.trim()) {
                violations.push(format!(
                    "{}:{} uses serde_json conversion in production service code; move protocol/object adapters to domain",
                    service.display(),
                    line_no + 1
                ));
            }

            if contains_ad_hoc_service_wiring(line.trim()) {
                violations.push(format!(
                    "{}:{} constructs repositories or services from self-held infrastructure in production service methods; move wiring to constructors/factories",
                    service.display(),
                    line_no + 1
                ));
            }

            if contains_service_error_policy(line.trim()) {
                violations.push(format!(
                    "{}:{} owns ErrorKind policy in production service code; move user-visible error contracts to domain helpers",
                    service.display(),
                    line_no + 1
                ));
            }
        }
    }

    assert!(violations.is_empty(), "service DDD boundary violations:\n{}", violations.join("\n"));
}

#[test]
fn repositories_do_not_reintroduce_domain_policy_helpers() {
    let repositories_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/repositories");
    let mut violations = Vec::new();

    for repository in rust_files_recursive(&repositories_dir) {
        let source = fs::read_to_string(&repository).expect("read repository source");
        for (line_no, line) in production_lines(&source) {
            let trimmed = line.trim();

            if contains_resource_slug_policy(trimmed) {
                violations.push(format!(
                    "{}:{} derives resource slugs in production repository code; resolve resource naming policy in domain/service",
                    repository.display(),
                    line_no + 1
                ));
            }

            if contains_cross_cutting_util_policy(trimmed) {
                violations.push(format!(
                    "{}:{} imports cross-cutting util policy in production repository code; move policy ownership to domain",
                    repository.display(),
                    line_no + 1
                ));
            }

            // DDD-3: deny-by-default. NO repository owns user-visible ErrorKind
            // policy in production code — error contracts belong in domain
            // helpers. This replaces the old per-file opt-in (which left repos
            // not named in any `is_*_repository` boundary unguarded); every
            // repository is now guarded. `REPO_ERROR_POLICY_ALLOWLIST` is the
            // shrink-only set of documented exceptions (empty today).
            if contains_repository_error_policy(trimmed) && !repo_error_policy_allowed(&repository, &repositories_dir) {
                violations.push(format!(
                    "{}:{} owns ErrorKind policy in production repository code; move user-visible error contracts to domain helpers",
                    repository.display(),
                    line_no + 1
                ));
            }
        }
    }

    assert!(violations.is_empty(), "repository DDD boundary violations:\n{}", violations.join("\n"));
}

/// Documented exceptions to the deny-by-default repository error-policy rule
/// (DDD-3). Empty today — every repository delegates user-visible error
/// contracts to domain helpers. Keyed by path relative to `src/repositories`.
const REPO_ERROR_POLICY_ALLOWLIST: &[&str] = &[];

fn repo_error_policy_allowed(path: &Path, repositories_dir: &Path) -> bool {
    let rel =
        path.strip_prefix(repositories_dir).unwrap_or(path).to_string_lossy().replace(std::path::MAIN_SEPARATOR, "/");
    REPO_ERROR_POLICY_ALLOWLIST.contains(&rel.as_str())
}

#[test]
fn mcp_entrypoint_does_not_reintroduce_ddd_boundary_leaks() {
    let mcp_file = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/mcp.rs");
    let source = fs::read_to_string(&mcp_file).expect("read MCP source");
    let mut violations = Vec::new();

    for (line_no, line) in production_lines(&source) {
        let trimmed = line.trim();
        if contains_raw_sql(trimmed) {
            violations.push(format!(
                "{}:{} uses raw SQL in production MCP entrypoint code; move persistence to repository/service boundaries",
                mcp_file.display(),
                line_no + 1
            ));
        }

        if contains_json_macro(trimmed) {
            violations.push(format!(
                "{}:{} uses json! in production MCP entrypoint code; move protocol payload construction to domain",
                mcp_file.display(),
                line_no + 1
            ));
        }

        if contains_route_error_policy(trimmed) {
            violations.push(format!(
                "{}:{} owns ErrorKind policy in production MCP entrypoint code; move error contracts to domain helpers",
                mcp_file.display(),
                line_no + 1
            ));
        }

        if contains_mcp_runtime_adapter_wiring(trimmed) {
            violations.push(format!(
                "{}:{} owns Docker runtime adapter wiring in production MCP entrypoint code; move runtime adapters to services",
                mcp_file.display(),
                line_no + 1
            ));
        }

        if contains_mcp_live_service_wiring(trimmed) {
            violations.push(format!(
                "{}:{} owns live service/repository wiring in production MCP entrypoint code; move live component wiring to services",
                mcp_file.display(),
                line_no + 1
            ));
        }
    }

    assert!(violations.is_empty(), "MCP DDD boundary violations:\n{}", violations.join("\n"));
}

#[test]
fn gateway_entrypoints_do_not_reintroduce_ddd_boundary_leaks() {
    let gateway_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/gateway");
    let mut violations = Vec::new();

    for gateway in rust_files_recursive(&gateway_dir) {
        let source = fs::read_to_string(&gateway).expect("read gateway source");
        for (line_no, line) in production_lines(&source) {
            let trimmed = line.trim();

            if contains_raw_sql(trimmed) {
                violations.push(format!(
                    "{}:{} uses raw SQL in production gateway code; move persistence to repository/service boundaries",
                    gateway.display(),
                    line_no + 1
                ));
            }

            if contains_json_macro(trimmed) {
                violations.push(format!(
                    "{}:{} uses json! in production gateway code; move WebSocket payload construction to domain",
                    gateway.display(),
                    line_no + 1
                ));
            }

            if contains_service_serde_adapter(trimmed) {
                violations.push(format!(
                    "{}:{} uses serde_json conversion in production gateway code; move protocol adapters to domain",
                    gateway.display(),
                    line_no + 1
                ));
            }

            if contains_repository_namespace_import(trimmed) || contains_repository_constructor(trimmed) {
                violations.push(format!(
                    "{}:{} owns repository wiring in production gateway code; move persistence access to services",
                    gateway.display(),
                    line_no + 1
                ));
            }

            if contains_route_error_policy(trimmed) {
                violations.push(format!(
                    "{}:{} owns ErrorKind policy in production gateway code; move error contracts to domain/service helpers",
                    gateway.display(),
                    line_no + 1
                ));
            }
        }
    }

    assert!(violations.is_empty(), "gateway DDD boundary violations:\n{}", violations.join("\n"));
}

#[test]
fn system_entrypoints_do_not_reintroduce_ddd_boundary_leaks() {
    let api_src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let entrypoints = [api_src.join("health.rs"), api_src.join("middleware.rs")];
    let mut violations = Vec::new();

    for entrypoint in entrypoints {
        let source = fs::read_to_string(&entrypoint).expect("read system entrypoint source");
        for (line_no, line) in production_lines(&source) {
            let trimmed = line.trim();

            if contains_json_macro(trimmed) {
                violations.push(format!(
                    "{}:{} uses json! in production system entrypoint code; move response construction to domain",
                    entrypoint.display(),
                    line_no + 1
                ));
            }

            if contains_route_error_policy(trimmed) {
                violations.push(format!(
                    "{}:{} owns ErrorKind policy in production system entrypoint code; move error contracts to domain helpers",
                    entrypoint.display(),
                    line_no + 1
                ));
            }

            if contains_system_response_contract_literal(trimmed) {
                violations.push(format!(
                    "{}:{} owns system response contract literals in production entrypoint code; move response contracts to domain",
                    entrypoint.display(),
                    line_no + 1
                ));
            }
        }
    }

    assert!(violations.is_empty(), "system entrypoint DDD boundary violations:\n{}", violations.join("\n"));
}

fn route_files(routes_dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<_> = fs::read_dir(routes_dir)
        .expect("read routes dir")
        .map(|entry| entry.expect("read routes entry").path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "rs"))
        .collect();
    files.sort();
    files
}

fn rust_files_recursive(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    collect_rust_files(dir, &mut files);
    files.sort();
    files
}

fn collect_rust_files(dir: &Path, files: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).expect("read rust source dir") {
        let path = entry.expect("read rust source entry").path();
        if path.is_dir() {
            collect_rust_files(&path, files);
        } else if path.extension().is_some_and(|extension| extension == "rs") && !is_test_support_path(&path) {
            files.push(path);
        }
    }
}

fn is_test_support_path(path: &Path) -> bool {
    path.components().any(|component| component.as_os_str() == "tests")
}

fn production_lines(source: &str) -> Vec<(usize, &str)> {
    let mut lines = Vec::new();
    let mut pending_cfg_test = false;
    let mut skip_depth: Option<i32> = None;

    for (index, line) in source.lines().enumerate() {
        if let Some(depth) = skip_depth.as_mut() {
            *depth += brace_delta(line);
            if *depth <= 0 {
                skip_depth = None;
            }
            continue;
        }

        let trimmed = line.trim();
        if pending_cfg_test {
            pending_cfg_test = false;
            let depth = brace_delta(line);
            if depth > 0 && !trimmed.ends_with(';') {
                skip_depth = Some(depth);
            }
            continue;
        }

        if is_test_cfg_attr(trimmed) {
            pending_cfg_test = true;
            continue;
        }

        lines.push((index + 1, line));
    }

    lines
}

fn brace_delta(line: &str) -> i32 {
    let opens = line.chars().filter(|character| *character == '{').count() as i32;
    let closes = line.chars().filter(|character| *character == '}').count() as i32;
    opens - closes
}

fn is_test_cfg_attr(line: &str) -> bool {
    line.starts_with("#[cfg(test)]")
        || line.starts_with(r#"#[cfg(feature = "test-support")]"#)
        || line.starts_with(r#"#[cfg(any(test, feature = "test-support"))]"#)
}

fn is_allowed_empty_json_default(line: &str) -> bool {
    matches!(line, "serde_json::json!({})" | "json!({})")
}

fn contains_json_macro(line: &str) -> bool {
    line.contains("serde_json::json!(") || line.contains("json!(")
}

fn contains_service_serde_adapter(line: &str) -> bool {
    ["serde_json::from_str(", "serde_json::from_value(", "serde_json::to_string(", "serde_json::to_value("]
        .iter()
        .any(|pattern| line.contains(pattern))
}

fn contains_ad_hoc_service_wiring(line: &str) -> bool {
    line.contains("Repository::new(self.")
}

fn contains_service_error_policy(line: &str) -> bool {
    if line.starts_with("///") || line.starts_with("//!") {
        return false;
    }

    line.contains("agentforge_core::ErrorKind")
        || (line.starts_with("use agentforge_core::") && line.contains("ErrorKind"))
        || (line.contains("ErrorKind::")
            && !line.contains("std::io::ErrorKind")
            && !line.contains("RefreshErrorKind::"))
}

fn contains_raw_sql(line: &str) -> bool {
    line.contains("sqlx::query")
        || line.contains("query_as::<")
        || line.contains("query_scalar")
        || line.contains("query!(")
        || line.contains("query_as!(")
        || line.contains("query_scalar!(")
}

fn contains_repository_namespace_import(line: &str) -> bool {
    line.starts_with("use crate::repositories::")
}

fn contains_repository_constructor(line: &str) -> bool {
    line.contains("Repository::new")
}

fn contains_runtime_state_wiring(line: &str) -> bool {
    [
        "state.config",
        "state.encryption_key",
        "state.llm_factory",
        "state.docker",
        "state.nats",
        "state.object_storage",
        "state.billing_gateway",
        "state.auth_callout",
        "state.email_sender",
        "state.jwt",
        "state.agent_command_bus",
        "state.pool.clone",
        "state.inflight_prompts",
        "state.redis",
        "state.cli_auth_memory_store",
        "state.context_resolver",
        "state.context_features",
    ]
    .iter()
    .any(|pattern| line.contains(pattern))
}

fn contains_route_service_constructor(line: &str) -> bool {
    line.contains("Service::new(")
        || line.contains("::from_pool(")
        || line.contains("from_app_config(")
        || line.contains("from_pool_and_app_config(")
        || line.contains("from_runtime(")
}

fn route_local_projection_name(line: &str) -> Option<&str> {
    let (_, after_struct) = line.split_once("struct ")?;
    let name = after_struct
        .split(|character: char| character.is_whitespace() || character == '{' || character == '(' || character == '<')
        .next()?;

    if ["Response", "Projection", "Snapshot", "View", "Dto", "Payload"].iter().any(|suffix| name.ends_with(suffix)) {
        Some(name)
    } else {
        None
    }
}

fn contains_runtime_policy_config(line: &str) -> bool {
    [
        "state.config.redis_url",
        "state.config.cli_auth_proxy_revoke_threshold",
        "state.config.oauth_mount_dir",
        "state.config.credential_sync_enabled",
        "state.config.container_anthropic_api_key",
        "state.config.container_google_api_key",
        "state.config.container_openai_api_key",
        "state.config.storage_max_file_size",
        "state.config.storage_max_files_per_session",
        "state.config.is_production",
        "state.config.app_url",
        "AgentContainerControlSettings::from_runtime",
    ]
    .iter()
    .any(|pattern| line.contains(pattern))
}

fn contains_identity_repository_wiring(line: &str) -> bool {
    [
        "ApiKeyRepository::new",
        "CliCredentialRepository::new",
        "GitCredentialRepository::new",
        "SshKeyRepository::new",
        "UserRepository::new",
    ]
    .iter()
    .any(|pattern| line.contains(pattern))
}

fn contains_agent_orchestration_repository_wiring(line: &str) -> bool {
    [
        "AgentRepository::new",
        "MessageRepository::new",
        "UserLlmConfigRepository::new",
        "OrchestrationTaskRepository::new",
        "ParticipantRepository::new",
        "TaskContextRepository::new",
        "ContextPreviewRepository::new",
        "workspace_root_from_env",
    ]
    .iter()
    .any(|pattern| line.contains(pattern))
}

fn contains_runtime_service_factory_wiring(line: &str) -> bool {
    [
        "AgentMessageService::new",
        "AgentPromptService::new",
        "AgentContainerLifecycleService::new",
        "OrchestrationService::new",
        "TaskContextService::new",
        "ContextPreviewService::new",
        "ContextEnvelopeService::new",
        "ContextApprovalService::new",
        "ContextFeatureService::new",
    ]
    .iter()
    .any(|pattern| line.contains(pattern))
}

fn contains_route_error_policy(line: &str) -> bool {
    line.contains("ErrorKind::")
        || line.contains("agentforge_core::ErrorKind")
        || (line.starts_with("use agentforge_core::") && line.contains("ErrorKind"))
}

fn contains_resource_slug_policy(line: &str) -> bool {
    line.contains("slugify(") || line.contains("ResourceSlugPolicy")
}

fn contains_cross_cutting_util_policy(line: &str) -> bool {
    line.contains("crate::util::")
}

fn contains_repository_error_policy(line: &str) -> bool {
    line.contains("ErrorKind::") || (line.starts_with("use agentforge_core::") && line.contains("ErrorKind"))
}

fn contains_system_response_contract_literal(line: &str) -> bool {
    ["\"healthy\"", "\"ready\"", "\"degraded\"", "\"INTERNAL_ERROR\"", "\"Internal server error\""]
        .iter()
        .any(|pattern| line.contains(pattern))
}

fn contains_mcp_runtime_adapter_wiring(line: &str) -> bool {
    [
        "use bollard::",
        "DockerMcpRuntimeBackend",
        "LiveDockerMcpRuntimeBackend",
        "DockerMcpAgentRuntime",
        "AttachContainerOptions",
        "CreateContainerOptions",
        "InspectContainerOptions",
        "LogsOptions",
        "RemoveContainerOptions",
        "StartContainerOptions",
        "ContainerCreateBody",
        "HostConfig",
        "StreamExt",
        "AsyncWriteExt",
    ]
    .iter()
    .any(|pattern| line.contains(pattern))
}

fn contains_mcp_live_service_wiring(line: &str) -> bool {
    [
        "use std::env",
        "use crate::repositories::",
        "McpAgentRepository",
        "McpAgentInsertRecord",
        "SqlxMcpAgentStore",
        "McpAgentService::new",
        "McpAgentRuntimeConfig",
        "docker_mcp_agent_runtime",
        "MCP_ENABLED",
        "MCP_TOKEN",
        "AGENTFORGE_WORKSPACE_ROOT",
        "CONTAINER_AGENT_IMAGE",
        "CONTAINER_IMAGE_",
        "CONTAINER_ANTHROPIC_API_KEY",
        "CONTAINER_OPENAI_API_KEY",
        "CONTAINER_GOOGLE_API_KEY",
    ]
    .iter()
    .any(|pattern| line.contains(pattern))
}

/// DDD keystone guard (#886-track DDD-1): the domain layer must stay independent
/// of the persistence layer. Domain types own pure business policies and
/// `Serialize`-derived projections; `sqlx::FromRow` derives, `agentforge_db`
/// row/entity imports, and `From<*Row>` adapters belong in services/repositories.
///
/// `DOMAIN_PERSISTENCE_BASELINE` records the EXACT current count of persistence
/// dependencies per dirty domain file so CI stays green while DDD-2 cleanup
/// lands. The baseline is a shrink-only ratchet:
/// - a file not in the baseline must have ZERO dependencies (stops the
///   self-propagating leak the audit found);
/// - a baselined file whose count GROWS fails the build (no new debt in an
///   already-dirty file — the gap codex flagged with a whole-file allowlist);
/// - a baselined file whose count DROPS fails too, forcing the entry to be
///   lowered (or removed at 0) so the ratchet tightens automatically.
const DOMAIN_PERSISTENCE_BASELINE: &[(&str, usize)] = &[
    // admin.rs/agent.rs/inbox.rs tightened in #896 F054: the `DeadEventRow`,
    // `AgentAggregate`, and inbox `From<InboxNotificationRow>` persistence
    // adapters moved to repositories/services. admin.rs drops 3 -> 2 (the
    // residual 2 are `agentforge_db::entities::{ImpersonationLog, User}`
    // imports); agent.rs and inbox.rs reach 0 and leave the baseline entirely.
    ("admin.rs", 2),
    ("context.rs", 5),
    ("context_preview.rs", 1),
    ("credential.rs", 3),
    ("observability.rs", 1),
    ("orchestration.rs", 2),
    ("project_clone.rs", 1),
    ("turn.rs", 1),
];

#[test]
fn domain_layer_stays_persistence_independent() {
    let domain_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/domain");
    let mut errors = Vec::new();

    let mut seen_baseline_keys = std::collections::BTreeSet::new();

    for file in rust_files_recursive(&domain_dir) {
        // Key by the path RELATIVE to the domain root (with forward slashes), not
        // the basename: the scan is recursive, so a future `domain/**/admin.rs`
        // must NOT inherit the flat `admin.rs` allowance (codex review P2).
        let rel =
            file.strip_prefix(&domain_dir).unwrap_or(&file).to_string_lossy().replace(std::path::MAIN_SEPARATOR, "/");
        let source = fs::read_to_string(&file).expect("read domain source");

        let count = domain_persistence_reference_count(&source);

        let baseline = DOMAIN_PERSISTENCE_BASELINE.iter().find(|(f, _)| *f == rel).map(|(_, c)| *c);
        if baseline.is_some() {
            seen_baseline_keys.insert(rel.clone());
        }
        match baseline {
            None if count > 0 => errors.push(format!(
                "{}: {count} persistence dependency(ies) in a clean domain file (agentforge_db / FromRow / From<*Row> / crate::repositories); move row adapters to services/repositories",
                file.display()
            )),
            Some(expected) if count > expected => errors.push(format!(
                "{}: persistence dependencies grew {expected} -> {count}; the DDD-1 baseline must only shrink (move the new coupling out of domain)",
                file.display()
            )),
            Some(expected) if count < expected => errors.push(format!(
                "{}: persistence dependencies dropped {expected} -> {count}; lower its DOMAIN_PERSISTENCE_BASELINE entry to {count} (or remove it when 0)",
                file.display()
            )),
            _ => {}
        }
    }

    // A baseline entry whose file was renamed/deleted leaves a stale allowance a
    // future same-path file could inherit — fail so the entry is removed (P3).
    for (key, _) in DOMAIN_PERSISTENCE_BASELINE {
        if !seen_baseline_keys.contains(*key) {
            errors.push(format!(
                "DOMAIN_PERSISTENCE_BASELINE has a stale entry `{key}` with no matching domain file; remove it so no future file inherits its allowance"
            ));
        }
    }

    assert!(errors.is_empty(), "domain purity (DDD-1) violations:\n{}", errors.join("\n"));
}

/// Count persistence references across a domain SOURCE file. Operates on whole
/// `use` statements (reconstructed across rustfmt line-splits) so widening a
/// braced import like `use agentforge_db::entities::{A, B, C}` always raises the
/// count even when formatted across multiple lines (codex review P2). Each
/// imported entity, each `FromRow`, each `From<*Row>` impl, and each
/// `crate::repositories::` reference counts once.
fn domain_persistence_reference_count(source: &str) -> usize {
    let lines: Vec<&str> = production_lines(source).into_iter().map(|(_, line)| line).collect();

    // Pass 1: collect the DB entity/row type names this file imports from
    // `agentforge_db`, so a new `impl From<ThatEntity>` adapter — even one whose
    // type is not `*Row`-suffixed (e.g. `User`, `ContextCandidate`) — is counted
    // as new persistence coupling in an already-dirty file (codex review P2).
    let mut imported_entities: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let mut index = 0;
    while index < lines.len() {
        let trimmed = lines[index].trim_start();
        if trimmed.starts_with("use ") {
            let mut statement = String::from(lines[index]);
            while !statement.contains(';') && index + 1 < lines.len() {
                index += 1;
                statement.push(' ');
                statement.push_str(lines[index]);
            }
            if statement.contains("agentforge_db") {
                collect_imported_entities(&statement, &mut imported_entities);
            }
        }
        index += 1;
    }

    // Pass 2: count references.
    let mut count = 0;
    let mut index = 0;
    while index < lines.len() {
        let line = lines[index];
        let trimmed = line.trim_start();
        if trimmed.starts_with("//") {
            index += 1;
            continue;
        }

        if trimmed.starts_with("use ") {
            // Reconstruct the full `use ...;` statement across continuation lines.
            let mut statement = String::from(line);
            while !statement.contains(';') && index + 1 < lines.len() {
                index += 1;
                statement.push(' ');
                statement.push_str(lines[index]);
            }
            count += count_use_statement_references(&statement);
        } else {
            count += count_non_use_line_references(line, &imported_entities);
        }
        index += 1;
    }

    count
}

/// The last `::`-segment of a path (the bare type name), with surrounding
/// whitespace and a leading `&` stripped.
fn last_type_segment(path: &str) -> &str {
    path.trim().trim_start_matches('&').trim().rsplit("::").next().unwrap_or(path).trim()
}

/// The entity/type identifiers (CamelCase, uppercase-initial) named anywhere in
/// an `agentforge_db` `use` statement. Counting identifiers rather than a single
/// `{...}` pair handles nested imports like
/// `use agentforge_db::{entities::{User, Workspace}, x::Row};` — module segments
/// (`entities`, `x`) are lowercase and ignored (codex review P2).
fn agentforge_db_entity_idents(statement: &str) -> impl Iterator<Item = &str> {
    statement
        .split(|character: char| !(character.is_alphanumeric() || character == '_'))
        .filter(|word| word.chars().next().is_some_and(char::is_uppercase))
}

/// Record the entity/row type names imported by an `agentforge_db` `use`
/// statement into `out`.
fn collect_imported_entities(statement: &str, out: &mut std::collections::BTreeSet<String>) {
    for name in agentforge_db_entity_idents(statement) {
        out.insert(name.to_string());
    }
}

/// References inside a reconstructed `use` statement: each `agentforge_db`
/// entity/type identifier counts once (a bare single import like
/// `agentforge_db::inbox_notifications::InboxNotificationRow` yields its one
/// terminal type); each `crate::repositories::` reference counts once.
fn count_use_statement_references(statement: &str) -> usize {
    let mut count = 0;
    if statement.contains("agentforge_db") {
        count += agentforge_db_entity_idents(statement).count();
    }
    count += statement.matches("crate::repositories::").count();
    count
}

/// References on a non-`use` production line. A `From`/`TryFrom` adapter whose
/// input is a DB row/entity (a `*Row` type, an `agentforge_db` path, or an
/// imported entity) counts once — and short-circuits so the `agentforge_db`
/// substring on the same line is not double-counted. Otherwise a bare
/// `agentforge_db` path (fn arg / inline type) counts once, plus each `FromRow`
/// and each `crate::repositories::` reference.
fn count_non_use_line_references(line: &str, imported_entities: &std::collections::BTreeSet<String>) -> usize {
    if let Some(inner) = from_impl_input_type(line) {
        let base = last_type_segment(inner);
        if inner.contains("agentforge_db") || base.ends_with("Row") || imported_entities.contains(base) {
            return 1;
        }
    }

    let mut count = 0;
    if line.contains("agentforge_db") {
        count += 1;
    }
    count += line.matches("FromRow").count();
    count += line.matches("crate::repositories::").count();
    count
}

/// Extract `X` from `impl From<X> for ...` / `impl TryFrom<X> for ...`.
fn from_impl_input_type(line: &str) -> Option<&str> {
    let rest = line.trim_start().strip_prefix("impl ")?;
    let after = rest.strip_prefix("From<").or_else(|| rest.strip_prefix("TryFrom<"))?;
    let end = after.find('>')?;
    Some(&after[..end])
}

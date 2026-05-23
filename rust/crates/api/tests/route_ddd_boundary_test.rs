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

            if is_context_candidate_repository(&repository) && contains_repository_error_policy(trimmed) {
                violations.push(format!(
                    "{}:{} owns context candidate error policy in production repository code; move error contracts to domain helpers",
                    repository.display(),
                    line_no + 1
                ));
            }

            if is_context_repository_policy_boundary(&repository) && contains_repository_error_policy(trimmed) {
                violations.push(format!(
                    "{}:{} owns context repository error policy in production repository code; move error contracts to domain helpers",
                    repository.display(),
                    line_no + 1
                ));
            }

            if is_orchestration_repository(&repository) && contains_repository_error_policy(trimmed) {
                violations.push(format!(
                    "{}:{} owns orchestration repository error policy in production repository code; move error contracts to domain helpers",
                    repository.display(),
                    line_no + 1
                ));
            }

            if is_agent_repository(&repository) && contains_repository_error_policy(trimmed) {
                violations.push(format!(
                    "{}:{} owns agent repository error policy in production repository code; move error contracts to domain helpers",
                    repository.display(),
                    line_no + 1
                ));
            }

            if is_identity_repository(&repository) && contains_repository_error_policy(trimmed) {
                violations.push(format!(
                    "{}:{} owns identity repository error policy in production repository code; move error contracts to domain helpers",
                    repository.display(),
                    line_no + 1
                ));
            }

            if is_resource_repository_policy_boundary(&repository) && contains_repository_error_policy(trimmed) {
                violations.push(format!(
                    "{}:{} owns resource repository error policy in production repository code; move error contracts to domain helpers",
                    repository.display(),
                    line_no + 1
                ));
            }

            if is_flat_repository_error_policy_boundary(&repository) && contains_repository_error_policy(trimmed) {
                violations.push(format!(
                    "{}:{} owns flat repository error policy in production repository code; move error contracts to domain helpers",
                    repository.display(),
                    line_no + 1
                ));
            }

            if is_remaining_repository_error_policy_boundary(&repository) && contains_repository_error_policy(trimmed) {
                violations.push(format!(
                    "{}:{} owns remaining repository error policy in production repository code; move error contracts to domain helpers",
                    repository.display(),
                    line_no + 1
                ));
            }
        }
    }

    assert!(violations.is_empty(), "repository DDD boundary violations:\n{}", violations.join("\n"));
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

fn is_context_candidate_repository(path: &Path) -> bool {
    path.components().any(|component| component.as_os_str() == "context_candidate")
}

fn is_context_repository_policy_boundary(path: &Path) -> bool {
    matches!(
        path.file_name().and_then(|file_name| file_name.to_str()),
        Some("context_envelope.rs" | "context_preview.rs" | "memory.rs")
    )
}

fn is_orchestration_repository(path: &Path) -> bool {
    path.components().any(|component| component.as_os_str() == "orchestration")
}

fn is_agent_repository(path: &Path) -> bool {
    path.components().any(|component| component.as_os_str() == "agent")
}

fn is_identity_repository(path: &Path) -> bool {
    path.components().any(|component| component.as_os_str() == "identity")
}

fn is_resource_repository_policy_boundary(path: &Path) -> bool {
    matches!(
        path.file_name().and_then(|file_name| file_name.to_str()),
        Some("member.rs" | "navigation.rs" | "permission.rs")
    )
}

fn is_flat_repository_error_policy_boundary(path: &Path) -> bool {
    let Some(file_name) = path.file_name().and_then(|file_name| file_name.to_str()) else {
        return false;
    };
    if matches!(
        file_name,
        "api_key.rs"
            | "attachment.rs"
            | "favorite.rs"
            | "feature_flag.rs"
            | "git.rs"
            | "license.rs"
            | "llm_config.rs"
            | "profile.rs"
            | "project.rs"
            | "prompt.rs"
            | "quota.rs"
            | "setting.rs"
            | "ssh_key.rs"
            | "tile.rs"
            | "voice.rs"
            | "workspace.rs"
    ) {
        return true;
    }
    false
}

fn is_remaining_repository_error_policy_boundary(path: &Path) -> bool {
    let Some(file_name) = path.file_name().and_then(|file_name| file_name.to_str()) else {
        return false;
    };
    if matches!(file_name, "admin.rs" | "billing.rs" | "dev_environment.rs" | "plugin.rs" | "usage_analytics.rs") {
        return true;
    }

    path.ends_with(Path::new("user/mod.rs"))
        || path.ends_with(Path::new("skill/mod.rs"))
        || path.ends_with(Path::new("skill/version.rs"))
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

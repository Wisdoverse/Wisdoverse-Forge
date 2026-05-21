use std::fs;
use std::path::{Path, PathBuf};

#[test]
fn route_handlers_do_not_reintroduce_ddd_boundary_leaks() {
    let routes_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/routes");
    let mut violations = Vec::new();

    for route in route_files(&routes_dir) {
        let source = fs::read_to_string(&route).expect("read route source");
        let production_source = production_section(&source);

        for (line_no, line) in production_source.lines().enumerate() {
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
        }
    }

    assert!(violations.is_empty(), "route DDD boundary violations:\n{}", violations.join("\n"));
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

fn production_section(source: &str) -> &str {
    source.split_once("#[cfg(test)]").map_or(source, |(production, _)| production)
}

fn is_allowed_empty_json_default(line: &str) -> bool {
    matches!(line, "serde_json::json!({})" | "json!({})")
}

fn contains_json_macro(line: &str) -> bool {
    line.contains("serde_json::json!(") || line.contains("json!(")
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

use std::collections::HashMap;
use std::env;
use std::sync::Arc;

use anyhow::{Context, anyhow};
use sqlx::PgPool;

use agentforge_platform::DockerClient;

use crate::services::mcp_agent::{McpAgentRuntimeConfig, McpAgentService, McpAgentTools};
use crate::services::mcp_agent_store::SqlxMcpAgentStore;
use crate::services::mcp_docker_runtime::docker_mcp_agent_runtime;

pub async fn build_live_mcp_components(
    pool: PgPool,
    docker: Option<Arc<DockerClient>>,
) -> anyhow::Result<Option<(String, Arc<dyn McpAgentTools>)>> {
    if !read_bool("MCP_ENABLED") {
        return Ok(None);
    }

    let token = read_required("MCP_TOKEN").context("MCP_ENABLED=true requires MCP_TOKEN")?;
    let docker = docker.ok_or_else(|| anyhow!("MCP_ENABLED=true requires Docker to be available"))?;
    let config = live_runtime_config();
    let store = SqlxMcpAgentStore::new(pool);
    let runtime = docker_mcp_agent_runtime(store.clone(), docker);
    let service = McpAgentService::new(store, runtime, config);

    Ok(Some((token, Arc::new(service))))
}

fn live_runtime_config() -> McpAgentRuntimeConfig {
    McpAgentRuntimeConfig {
        workspace_root: env::var("AGENTFORGE_WORKSPACE_ROOT")
            .unwrap_or_else(|_| "/data/agentforge/workspaces".to_string()),
        default_image: env::var("CONTAINER_AGENT_IMAGE").unwrap_or_else(|_| "agentforge-agent:latest".to_string()),
        tool_images: collect_tool_images(),
        system_api_keys: collect_system_api_keys(),
    }
}

fn collect_tool_images() -> HashMap<String, String> {
    HashMap::from_iter(
        [
            ("claude", env::var("CONTAINER_IMAGE_CLAUDE").ok()),
            ("opencode", env::var("CONTAINER_IMAGE_OPENCODE").ok()),
            ("codex", env::var("CONTAINER_IMAGE_CODEX").ok()),
            ("gemini", env::var("CONTAINER_IMAGE_GEMINI").ok()),
        ]
        .into_iter()
        .filter_map(|(tool, image)| {
            image.filter(|value| !value.trim().is_empty()).map(|value| (tool.to_string(), value))
        }),
    )
}

fn collect_system_api_keys() -> HashMap<String, String> {
    HashMap::from_iter(
        [
            ("ANTHROPIC_API_KEY", env::var("CONTAINER_ANTHROPIC_API_KEY").ok()),
            ("OPENAI_API_KEY", env::var("CONTAINER_OPENAI_API_KEY").ok()),
            ("GEMINI_API_KEY", env::var("CONTAINER_GOOGLE_API_KEY").ok()),
        ]
        .into_iter()
        .filter_map(|(name, value)| {
            value.filter(|entry| !entry.trim().is_empty()).map(|entry| (name.to_string(), entry))
        }),
    )
}

fn read_bool(name: &str) -> bool {
    matches!(env::var(name).ok().as_deref(), Some("1") | Some("true") | Some("TRUE") | Some("yes") | Some("on"))
}

fn read_required(name: &str) -> anyhow::Result<String> {
    let value = env::var(name).with_context(|| format!("missing environment variable {name}"))?;
    if value.trim().is_empty() {
        return Err(anyhow!("environment variable {name} is empty"));
    }
    Ok(value)
}

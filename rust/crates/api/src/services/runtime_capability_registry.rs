//! Runtime capability registry backed by the typed `core` matrix.

use std::collections::HashMap;
use std::sync::Arc;

use agentforge_core::{AppResult, CliToolKind, RuntimeCapability, RuntimeKind};
use tokio::sync::RwLock;

use crate::domain::runtime_capability::{
    RuntimeCapabilityKey, RuntimeCapabilityRecord, RuntimeCapabilityRegistryPolicy,
};
use crate::repositories::runtime_capability::{RuntimeCapabilityRepository, RuntimeCapabilityRow};

#[derive(Clone)]
pub struct RuntimeCapabilityRegistryService {
    repository: RuntimeCapabilityRepository,
    cache: Arc<RwLock<HashMap<RuntimeCapabilityKey, RuntimeCapability>>>,
}

impl RuntimeCapabilityRegistryService {
    pub fn new(repository: RuntimeCapabilityRepository) -> Self {
        Self { repository, cache: Arc::new(RwLock::new(HashMap::new())) }
    }

    /// Platform API startup entrypoint. Empty tables are seeded from code; any
    /// existing malformed or divergent row fails startup before cache swap.
    pub async fn refresh_from_code(&self) -> AppResult<()> {
        let expected = RuntimeCapabilityRegistryPolicy::expected_profiles();
        if self.repository.count().await? == 0 {
            self.repository.insert_seed_profiles(&RuntimeCapability::all()).await?;
        }

        let next_cache = self.load_validated_cache(&expected).await?;
        *self.cache.write().await = next_cache;
        Ok(())
    }

    /// Lookup never reaches into the database. Sidecars and runtime paths get
    /// the last startup-validated cache or a conservative fallback.
    pub async fn for_cli_tool(&self, cli_tool: CliToolKind, runtime_kind: RuntimeKind) -> RuntimeCapability {
        let key = RuntimeCapabilityKey::new(cli_tool, runtime_kind);
        if let Some(profile) = self.cache.read().await.get(&key).cloned() {
            return profile;
        }

        metrics::counter!(
            "runtime_capability_fallback_total",
            "cli_tool" => cli_tool.as_str(),
            "runtime_kind" => runtime_kind.as_str(),
        )
        .increment(1);
        RuntimeCapability::fallback_for_cli_tool(cli_tool, runtime_kind)
    }

    async fn load_validated_cache(
        &self,
        expected: &HashMap<RuntimeCapabilityKey, RuntimeCapability>,
    ) -> AppResult<HashMap<RuntimeCapabilityKey, RuntimeCapability>> {
        let rows = self.repository.list_all().await?;
        RuntimeCapabilityRegistryPolicy::validate_records(
            rows.into_iter().map(runtime_capability_record).collect(),
            expected,
        )
    }
}

fn runtime_capability_record(row: RuntimeCapabilityRow) -> RuntimeCapabilityRecord {
    RuntimeCapabilityRecord {
        cli_tool: row.cli_tool,
        runtime_kind: row.runtime_kind,
        max_context_tokens: row.max_context_tokens,
        supports_skills_mount: row.supports_skills_mount,
        supports_hooks: row.supports_hooks,
        supports_subagents: row.supports_subagents,
        supports_mcp_bridge: row.supports_mcp_bridge,
        supports_terminal: row.supports_terminal,
        capability_profile: row.capability_profile,
    }
}

//! Runtime capability registry backed by the typed `core` matrix.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use agentforge_core::{AppError, AppResult, CliToolKind, ErrorKind, RuntimeCapability, RuntimeKind};
use anyhow::anyhow;
use tokio::sync::RwLock;

use crate::repositories::runtime_capability::{RuntimeCapabilityRepository, RuntimeCapabilityRow};

const RESEED_MIGRATION: &str = "051_runtime_capabilities.sql";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct RuntimeCapabilityKey {
    cli_tool: CliToolKind,
    runtime_kind: RuntimeKind,
}

impl RuntimeCapabilityKey {
    fn new(cli_tool: CliToolKind, runtime_kind: RuntimeKind) -> Self {
        Self { cli_tool, runtime_kind }
    }

    fn label(self) -> String {
        format!("{}/{}", self.cli_tool, self.runtime_kind)
    }
}

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
        let expected = expected_profiles();
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
        let mut next = HashMap::with_capacity(expected.len());
        let mut seen = HashSet::with_capacity(expected.len());

        for row in rows {
            let (key, profile) = validated_profile(row, expected)?;
            if !seen.insert(key) {
                return Err(startup_error(format!(
                    "duplicate runtime_capabilities row for {}; run migration {RESEED_MIGRATION}",
                    key.label()
                )));
            }
            next.insert(key, profile);
        }

        for key in expected.keys().copied() {
            if !seen.contains(&key) {
                return Err(startup_error(format!(
                    "runtime_capabilities row for {} is missing; run migration {RESEED_MIGRATION}",
                    key.label()
                )));
            }
        }

        Ok(next)
    }
}

fn expected_profiles() -> HashMap<RuntimeCapabilityKey, RuntimeCapability> {
    RuntimeCapability::all()
        .into_iter()
        .filter_map(|profile| {
            profile.cli_tool.map(|cli_tool| (RuntimeCapabilityKey::new(cli_tool, profile.runtime_kind), profile))
        })
        .collect()
}

fn validated_profile(
    row: RuntimeCapabilityRow,
    expected: &HashMap<RuntimeCapabilityKey, RuntimeCapability>,
) -> AppResult<(RuntimeCapabilityKey, RuntimeCapability)> {
    let row_key = row_key(&row)?;
    let profile = serde_json::from_value::<RuntimeCapability>(row.capability_profile.clone())
        .map_err(|err| startup_error(format!("invalid capability_profile for {}: {err}", row_key.label())))?;
    let Some(profile_cli_tool) = profile.cli_tool else {
        return Err(startup_error(format!("invalid capability_profile for {}: cli_tool is required", row_key.label())));
    };
    let profile_key = RuntimeCapabilityKey::new(profile_cli_tool, profile.runtime_kind);
    if profile_key != row_key {
        return Err(startup_error(format!(
            "runtime_capabilities row key {} does not match capability_profile key {}; run migration {RESEED_MIGRATION}",
            row_key.label(),
            profile_key.label()
        )));
    }
    if !row_scalars_match_profile(&row, &profile) {
        return Err(startup_error(format!(
            "runtime_capabilities scalar columns for {} diverge from capability_profile; run migration {RESEED_MIGRATION}",
            row_key.label()
        )));
    }

    match expected.get(&row_key) {
        Some(expected_profile) if expected_profile == &profile => Ok((row_key, profile)),
        Some(_) => Err(startup_error(format!(
            "runtime_capabilities row for {} diverges from code; run migration {RESEED_MIGRATION} to reseed from RuntimeCapability::all()",
            row_key.label()
        ))),
        None => Err(startup_error(format!(
            "runtime_capabilities row for {} is not part of RuntimeCapability::all(); run migration {RESEED_MIGRATION}",
            row_key.label()
        ))),
    }
}

fn row_key(row: &RuntimeCapabilityRow) -> AppResult<RuntimeCapabilityKey> {
    let cli_tool = CliToolKind::parse_legacy(&row.cli_tool)
        .map_err(|err| startup_error(format!("invalid runtime_capabilities cli_tool '{}': {err}", row.cli_tool)))?;
    let runtime_kind = RuntimeKind::parse_legacy(&row.runtime_kind).map_err(|err| {
        startup_error(format!("invalid runtime_capabilities runtime_kind '{}': {err}", row.runtime_kind))
    })?;
    Ok(RuntimeCapabilityKey::new(cli_tool, runtime_kind))
}

fn row_scalars_match_profile(row: &RuntimeCapabilityRow, profile: &RuntimeCapability) -> bool {
    row.max_context_tokens == i32::try_from(profile.max_context_tokens).unwrap_or(i32::MAX)
        && row.supports_skills_mount == profile.supports_skills_mount
        && row.supports_hooks == profile.supports_hooks
        && row.supports_subagents == profile.supports_subagents
        && row.supports_mcp_bridge == profile.supports_mcp_bridge
        && row.supports_terminal == profile.supports_terminal
}

fn startup_error(message: String) -> AppError {
    ErrorKind::Internal(anyhow!("{message}")).into()
}

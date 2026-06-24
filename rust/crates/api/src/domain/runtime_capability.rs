//! Domain policies for runtime capability registry consistency.

use std::collections::{HashMap, HashSet};

use agentforge_core::{AppError, AppResult, CliToolKind, ErrorKind, RuntimeCapability, RuntimeKind};
use anyhow::anyhow;
use serde_json::Value;

const RESEED_MIGRATION: &str = "051_runtime_capabilities.sql";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct RuntimeCapabilityKey {
    pub(crate) cli_tool: CliToolKind,
    pub(crate) runtime_kind: RuntimeKind,
}

impl RuntimeCapabilityKey {
    pub(crate) fn new(cli_tool: CliToolKind, runtime_kind: RuntimeKind) -> Self {
        Self { cli_tool, runtime_kind }
    }

    pub(crate) fn label(self) -> String {
        format!("{}/{}", self.cli_tool, self.runtime_kind)
    }
}

pub(crate) struct RuntimeCapabilityRecord {
    pub(crate) cli_tool: String,
    pub(crate) runtime_kind: String,
    pub(crate) max_context_tokens: i32,
    pub(crate) supports_skills_mount: bool,
    pub(crate) supports_hooks: bool,
    pub(crate) supports_subagents: bool,
    pub(crate) supports_mcp_bridge: bool,
    pub(crate) supports_terminal: bool,
    pub(crate) capability_profile: Value,
}

pub(crate) struct RuntimeCapabilityRegistryPolicy;

impl RuntimeCapabilityRegistryPolicy {
    pub(crate) fn expected_profiles() -> HashMap<RuntimeCapabilityKey, RuntimeCapability> {
        RuntimeCapability::all()
            .into_iter()
            .filter_map(|profile| {
                profile.cli_tool.map(|cli_tool| (RuntimeCapabilityKey::new(cli_tool, profile.runtime_kind), profile))
            })
            .collect()
    }

    pub(crate) fn validate_records(
        rows: Vec<RuntimeCapabilityRecord>,
        expected: &HashMap<RuntimeCapabilityKey, RuntimeCapability>,
    ) -> AppResult<HashMap<RuntimeCapabilityKey, RuntimeCapability>> {
        let mut next = HashMap::with_capacity(expected.len());
        let mut seen = HashSet::with_capacity(expected.len());

        for row in rows {
            let (key, profile) = validate_record(row, expected)?;
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

fn validate_record(
    row: RuntimeCapabilityRecord,
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

fn row_key(row: &RuntimeCapabilityRecord) -> AppResult<RuntimeCapabilityKey> {
    let cli_tool = CliToolKind::parse_legacy(&row.cli_tool)
        .map_err(|err| startup_error(format!("invalid runtime_capabilities cli_tool '{}': {err}", row.cli_tool)))?;
    let runtime_kind = RuntimeKind::parse_legacy(&row.runtime_kind).map_err(|err| {
        startup_error(format!("invalid runtime_capabilities runtime_kind '{}': {err}", row.runtime_kind))
    })?;
    Ok(RuntimeCapabilityKey::new(cli_tool, runtime_kind))
}

fn row_scalars_match_profile(row: &RuntimeCapabilityRecord, profile: &RuntimeCapability) -> bool {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn record_from_profile(profile: &RuntimeCapability) -> RuntimeCapabilityRecord {
        let cli_tool = profile.cli_tool.expect("test profile should have cli_tool");
        RuntimeCapabilityRecord {
            cli_tool: cli_tool.as_str().to_string(),
            runtime_kind: profile.runtime_kind.as_str().to_string(),
            max_context_tokens: i32::try_from(profile.max_context_tokens).expect("test profile fits i32"),
            supports_skills_mount: profile.supports_skills_mount,
            supports_hooks: profile.supports_hooks,
            supports_subagents: profile.supports_subagents,
            supports_mcp_bridge: profile.supports_mcp_bridge,
            supports_terminal: profile.supports_terminal,
            capability_profile: serde_json::to_value(profile).expect("serialize test profile"),
        }
    }

    fn internal_message(err: AppError) -> String {
        match err.kind {
            ErrorKind::Internal(source) => source.to_string(),
            other => panic!("expected internal error, got {other}"),
        }
    }

    #[test]
    fn registry_policy_accepts_builtin_capability_matrix() {
        let expected = RuntimeCapabilityRegistryPolicy::expected_profiles();
        let rows = RuntimeCapability::all().iter().map(record_from_profile).collect();

        let cache = RuntimeCapabilityRegistryPolicy::validate_records(rows, &expected).expect("matrix should validate");

        assert_eq!(cache.len(), expected.len());
        let key = RuntimeCapabilityKey::new(CliToolKind::Codex, RuntimeKind::Container);
        assert_eq!(cache.get(&key), expected.get(&key));
    }

    #[test]
    fn registry_policy_rejects_profile_key_mismatch() {
        let expected = RuntimeCapabilityRegistryPolicy::expected_profiles();
        let profile = RuntimeCapability::for_cli_tool(CliToolKind::Codex, RuntimeKind::Container);
        let mut row = record_from_profile(&profile);
        row.cli_tool = CliToolKind::Claude.as_str().to_string();

        let err =
            RuntimeCapabilityRegistryPolicy::validate_records(vec![row], &expected).expect_err("mismatch should fail");

        assert!(
            internal_message(err)
                .contains("row key claude/container does not match capability_profile key codex/container")
        );
    }

    #[test]
    fn registry_policy_rejects_scalar_drift() {
        let expected = RuntimeCapabilityRegistryPolicy::expected_profiles();
        let profile = RuntimeCapability::for_cli_tool(CliToolKind::Codex, RuntimeKind::Container);
        let mut row = record_from_profile(&profile);
        row.max_context_tokens -= 1;

        let err =
            RuntimeCapabilityRegistryPolicy::validate_records(vec![row], &expected).expect_err("drift should fail");

        assert!(internal_message(err).contains("scalar columns for codex/container diverge from capability_profile"));
    }

    #[test]
    fn registry_policy_rejects_missing_expected_profile() {
        let expected = RuntimeCapabilityRegistryPolicy::expected_profiles();
        let rows = RuntimeCapability::all()
            .into_iter()
            .filter(|profile| {
                profile.cli_tool != Some(CliToolKind::Codex) || profile.runtime_kind != RuntimeKind::Container
            })
            .map(|profile| record_from_profile(&profile))
            .collect();

        let err =
            RuntimeCapabilityRegistryPolicy::validate_records(rows, &expected).expect_err("missing row should fail");

        assert!(internal_message(err).contains("runtime_capabilities row for codex/container is missing"));
    }
}

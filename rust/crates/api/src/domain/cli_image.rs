//! CLI agent-image auto-updater read projections (deployment-global).
//!
//! Pure shaping of the worker's in-memory per-tool state + the cross-tenant
//! container counts into the response the admin status endpoint serves. Holds no
//! I/O and no tenant scope — image state is per host, not per org.

use std::collections::BTreeMap;

use agentforge_jobs::{CliImagePruneSummary, CliToolImageState};
use serde::Serialize;
use serde_json::{Value, json};

/// One pollable Container CLI tool's current image-update state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CliImageToolStatus {
    pub tool: String,
    /// `pending` (no tick yet) | `up_to_date` | `updated` | `failed`.
    pub state: String,
    /// Manifest digest of the locally-pulled GHCR ref (`None` until first pull).
    pub local_digest: Option<String>,
    /// Manifest digest currently advertised by the registry.
    pub remote_digest: Option<String>,
    pub last_checked_unix: Option<i64>,
    pub last_updated_unix: Option<i64>,
    pub last_error: Option<String>,
    /// Agents that currently have an associated container for this tool. A rough
    /// blast-radius hint for an operator weighing a manual roll; it does NOT
    /// assert which image digest each live container actually booted from.
    pub agents_with_container: i64,
}

/// Result of the most recent superseded-image prune sweep (default-off).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CliImagePruneStatus {
    /// Operator INTENT from deployment config (`CLI_IMAGE_PRUNE_ENABLED`). True
    /// here does NOT imply a sweep has run — prune lives inside the updater
    /// loop, so it is inert until `auto_update_enabled` is also true. Pair with
    /// `last_run_unix` to distinguish "off by config" / "on but never ran" /
    /// "on and ran".
    pub enabled: bool,
    /// `None` until a sweep has actually executed. `enabled && last_run_unix ==
    /// None` means prune is configured on but the updater hasn't run it yet
    /// (commonly because auto-update is off, so the worker never spawned).
    pub last_run_unix: Option<i64>,
    /// Candidate superseded agent images considered in the last sweep.
    pub scanned: u64,
    pub removed: u64,
    /// Left intact because a container still references them.
    pub skipped_in_use: u64,
    /// Left intact because Docker returned 409 (still tagged / has a child).
    pub skipped_conflict: u64,
    pub errors: u64,
    pub last_error: Option<String>,
}

impl CliImagePruneStatus {
    /// Combine the operator's CONFIGURED intent with the worker's last-sweep
    /// summary. `configured` (not `summary.enabled`) drives the surfaced
    /// `enabled` flag, so a "prune on but auto-update off" misconfiguration is
    /// reported as enabled-with-no-run rather than the reassuring "off".
    fn from_summary(configured: bool, summary: CliImagePruneSummary) -> Self {
        Self {
            enabled: configured,
            last_run_unix: summary.last_run_unix,
            scanned: summary.scanned,
            removed: summary.removed,
            skipped_in_use: summary.skipped_in_use,
            skipped_conflict: summary.skipped_conflict,
            errors: summary.errors,
            last_error: summary.last_error,
        }
    }
}

/// The full status report served by `GET /admin/cli-images`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CliImageStatusReport {
    /// Whether the background auto-updater is enabled in deployment config. When
    /// false, every tool stays `pending` (the worker never ticks).
    pub auto_update_enabled: bool,
    /// Effective poll cadence in seconds — the configured value after the
    /// worker's >= 60s floor, so it matches the rate the worker actually runs.
    pub poll_interval_secs: u64,
    /// Registry base the updater pulls overlays from.
    pub registry: String,
    /// Image tag the updater tracks.
    pub image_tag: String,
    pub tools: Vec<CliImageToolStatus>,
    /// Superseded-image prune result (default-off).
    pub prune: CliImagePruneStatus,
}

impl CliImageStatusReport {
    /// Merge the worker's per-tool snapshot with the cross-tenant container
    /// counts and deployment config. Driven by `pollable_tools` (the canonical
    /// poll set) so a never-yet-checked tool still appears as `pending` rather
    /// than vanishing from the report.
    #[allow(clippy::too_many_arguments)]
    pub fn build(
        auto_update_enabled: bool,
        poll_interval_secs: u64,
        registry: String,
        image_tag: String,
        pollable_tools: &[&str],
        snapshot: &BTreeMap<String, CliToolImageState>,
        container_counts: &BTreeMap<String, i64>,
        prune_configured: bool,
        prune: CliImagePruneSummary,
    ) -> Self {
        let tools = pollable_tools
            .iter()
            .map(|tool| {
                let recorded = snapshot.get(*tool);
                CliImageToolStatus {
                    tool: (*tool).to_string(),
                    state: recorded
                        .map(|s| s.state.clone())
                        .filter(|s| !s.is_empty())
                        .unwrap_or_else(|| "pending".to_string()),
                    local_digest: recorded.and_then(|s| s.local_digest.clone()),
                    remote_digest: recorded.and_then(|s| s.remote_digest.clone()),
                    last_checked_unix: recorded.and_then(|s| s.last_checked_unix),
                    last_updated_unix: recorded.and_then(|s| s.last_updated_unix),
                    last_error: recorded.and_then(|s| s.last_error.clone()),
                    agents_with_container: container_counts.get(*tool).copied().unwrap_or(0),
                }
            })
            .collect();

        Self {
            auto_update_enabled,
            poll_interval_secs,
            registry,
            image_tag,
            tools,
            prune: CliImagePruneStatus::from_summary(prune_configured, prune),
        }
    }
}

/// `{ ok: true, data: <report> }` envelope, matching the admin surface style.
pub(crate) fn cli_image_status_response(report: CliImageStatusReport) -> Value {
    json!({ "ok": true, "data": report })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(s: &str) -> CliToolImageState {
        CliToolImageState {
            state: s.to_string(),
            local_digest: Some("sha256:local".into()),
            remote_digest: Some("sha256:remote".into()),
            last_checked_unix: Some(1_700_000_000),
            last_updated_unix: None,
            last_error: None,
        }
    }

    #[test]
    fn unchecked_tool_is_pending_with_zero_agents() {
        let report = CliImageStatusReport::build(
            false,
            900,
            "ghcr.io/x".into(),
            "latest".into(),
            &["codex", "gemini"],
            &BTreeMap::new(),
            &BTreeMap::new(),
            false,
            CliImagePruneSummary::default(),
        );
        assert_eq!(report.tools.len(), 2);
        assert!(report.tools.iter().all(|t| t.state == "pending"));
        assert!(report.tools.iter().all(|t| t.agents_with_container == 0));
        assert!(!report.auto_update_enabled);
        // prune defaults to disabled/zeroed.
        assert!(!report.prune.enabled);
        assert_eq!(report.prune.removed, 0);
        assert_eq!(report.prune.last_run_unix, None);
    }

    #[test]
    fn prune_enabled_in_config_reports_enabled_even_before_a_sweep_runs() {
        // The misconfiguration case: prune configured ON but the worker never
        // ran a sweep (e.g. auto-update off). Must report enabled=true with no
        // last_run, NOT the reassuring "off".
        let report = CliImageStatusReport::build(
            false,
            900,
            "ghcr.io/x".into(),
            "latest".into(),
            &["codex"],
            &BTreeMap::new(),
            &BTreeMap::new(),
            true,
            CliImagePruneSummary::default(),
        );
        assert!(report.prune.enabled, "configured intent must surface even before a sweep ran");
        assert_eq!(report.prune.last_run_unix, None, "no sweep ran yet");
    }

    #[test]
    fn merges_snapshot_and_counts_by_tool() {
        let mut snap = BTreeMap::new();
        snap.insert("codex".to_string(), state("up_to_date"));
        let mut counts = BTreeMap::new();
        counts.insert("codex".to_string(), 4);

        let report = CliImageStatusReport::build(
            true,
            600,
            "ghcr.io/x".into(),
            "latest".into(),
            &["codex", "gemini"],
            &snap,
            &counts,
            true,
            CliImagePruneSummary {
                enabled: true,
                removed: 3,
                last_run_unix: Some(1_700_000_000),
                ..Default::default()
            },
        );

        let codex = report.tools.iter().find(|t| t.tool == "codex").unwrap();
        assert_eq!(codex.state, "up_to_date");
        assert_eq!(codex.agents_with_container, 4);
        assert_eq!(codex.remote_digest.as_deref(), Some("sha256:remote"));
        assert!(report.prune.enabled);
        assert_eq!(report.prune.removed, 3);

        // gemini has no snapshot or count → pending / 0, never dropped.
        let gemini = report.tools.iter().find(|t| t.tool == "gemini").unwrap();
        assert_eq!(gemini.state, "pending");
        assert_eq!(gemini.agents_with_container, 0);
    }
}

//! CLI agent-image auto-updater read projections (deployment-global).
//!
//! Pure shaping of the worker's in-memory per-tool state + the cross-tenant
//! container counts into the response the admin status endpoint serves. Holds no
//! I/O and no tenant scope — image state is per host, not per org.

use std::collections::BTreeMap;

use agentforge_core::{AppError, AppResult, ErrorKind};
use agentforge_jobs::{CliImagePruneSummary, CliToolImageState, pollable_tool_names, update_mode_for};
use serde::Serialize;
use serde_json::{Value, json};
use uuid::Uuid;

/// One reported Container CLI tool's current image-update state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CliImageToolStatus {
    pub tool: String,
    /// `pending` (no tick yet) | `up_to_date` | `updated` | `failed`; the
    /// local-build tool (`claude`) additionally reports `update_available`.
    pub state: String,
    /// `registry` (pulled + re-tagged from GHCR) or `local_build` (claude —
    /// no public image, built server-side from npm).
    pub update_mode: String,
    /// Manifest digest of the locally-pulled GHCR ref (`None` until first pull;
    /// always `None` for local-build tools).
    pub local_digest: Option<String>,
    /// Manifest digest currently advertised by the registry.
    pub remote_digest: Option<String>,
    /// Local-build tools: CLI version baked into the local image (`None` =
    /// unknown — image missing or unlabeled).
    pub local_version: Option<String>,
    /// Local-build tools: latest version published on the npm registry.
    pub remote_version: Option<String>,
    /// True while a server-side local build is running for this tool.
    pub building: bool,
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
    /// Whether the sweep auto-builds the claude image on a newer npm version
    /// (`CLI_IMAGE_CLAUDE_AUTO_BUILD`). Operator INTENT — surfaced even when the
    /// worker never ran, so the panel can say "auto-build on" truthfully.
    pub claude_auto_build_enabled: bool,
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
    /// counts and deployment config. Driven by `reported_tools` (the registry
    /// poll set plus the local-build `claude`) so a never-yet-checked tool
    /// still appears as `pending` rather than vanishing from the report.
    #[allow(clippy::too_many_arguments)]
    pub fn build(
        auto_update_enabled: bool,
        claude_auto_build_enabled: bool,
        poll_interval_secs: u64,
        registry: String,
        image_tag: String,
        reported_tools: &[&str],
        snapshot: &BTreeMap<String, CliToolImageState>,
        container_counts: &BTreeMap<String, i64>,
        prune_configured: bool,
        prune: CliImagePruneSummary,
    ) -> Self {
        let tools = reported_tools
            .iter()
            .map(|tool| {
                let recorded = snapshot.get(*tool);
                CliImageToolStatus {
                    tool: (*tool).to_string(),
                    state: recorded
                        .map(|s| s.state.clone())
                        .filter(|s| !s.is_empty())
                        .unwrap_or_else(|| "pending".to_string()),
                    // Derived from the canonical per-tool mode, not the recorded
                    // snapshot, so a pending (never-checked) claude row is still
                    // labeled `local_build`.
                    update_mode: update_mode_for(tool).to_string(),
                    local_digest: recorded.and_then(|s| s.local_digest.clone()),
                    remote_digest: recorded.and_then(|s| s.remote_digest.clone()),
                    local_version: recorded.and_then(|s| s.local_version.clone()),
                    remote_version: recorded.and_then(|s| s.remote_version.clone()),
                    building: recorded.map(|s| s.building).unwrap_or(false),
                    last_checked_unix: recorded.and_then(|s| s.last_checked_unix),
                    last_updated_unix: recorded.and_then(|s| s.last_updated_unix),
                    last_error: recorded.and_then(|s| s.last_error.clone()),
                    agents_with_container: container_counts.get(*tool).copied().unwrap_or(0),
                }
            })
            .collect();

        Self {
            auto_update_enabled,
            claude_auto_build_enabled,
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

// ---------------------------------------------------------------------------
// Operator-initiated roll (POST /admin/cli-images/{tool}/roll)
// ---------------------------------------------------------------------------

/// Which tools may be rolled. The roll path is destructive (it interrupts
/// running agents), so the allowlist is asserted in the DOMAIN — once here, and
/// re-asserted in the service — never trusting only the route.
pub(crate) struct RollToolPolicy;

impl RollToolPolicy {
    /// Accept only a canonical pollable tool. `claude` (no public image, never
    /// auto-managed) and unknown values are rejected with 422 — NOT 404, since
    /// the route path matched; the value is just not a rollable tool.
    pub(crate) fn ensure_rollable(tool: &str) -> AppResult<()> {
        if pollable_tool_names().contains(&tool) {
            Ok(())
        } else {
            Err(ErrorKind::Unprocessable(format!(
                "'{tool}' is not a rollable CLI tool; expected one of: {}",
                pollable_tool_names().join(", ")
            ))
            .into())
        }
    }
}

/// Per-agent outcome of a roll. `error` is a client-safe message (never an
/// internal error), present only when `ok` is false.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RollAgentResult {
    pub agent_id: Uuid,
    pub ok: bool,
    /// Only meaningful when `ok == false`: `true` means the agent's container was
    /// confirmed stopped+removed but the respawn failed (so it is now DOWN —
    /// restart it); `false` means the stop itself did not complete cleanly, so the
    /// post-condition is UNCONFIRMED (the agent may still be running on the
    /// previous image, or a partial stop may have already brought it down). The
    /// UI tells the operator to check the Agents view rather than asserting either.
    pub stopped: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl RollAgentResult {
    pub(crate) fn respawned(agent_id: Uuid) -> Self {
        Self { agent_id, ok: true, stopped: false, error: None }
    }

    /// Respawn failed after the container was stopped+removed → agent is now down.
    pub(crate) fn failed_now_stopped(agent_id: Uuid, error: String) -> Self {
        Self { agent_id, ok: false, stopped: true, error: Some(error) }
    }

    /// Stop did not complete cleanly → post-condition unconfirmed (may still be
    /// running on the previous image, or already down from a partial stop).
    pub(crate) fn failed_still_running(agent_id: Uuid, error: String) -> Self {
        Self { agent_id, ok: false, stopped: false, error: Some(error) }
    }
}

/// Map a roll failure to a CLIENT-SAFE message. The per-agent error is returned
/// in a `200` report body, so it must not leak internals: an `Internal`
/// (Docker/anyhow/DB) error collapses to a generic line (the full error is
/// logged server-side), while typed domain errors keep their operator-facing
/// message. Mirrors the `AppError::IntoResponse` redaction contract.
pub(crate) fn client_safe_roll_error(err: &AppError) -> String {
    match &err.kind {
        ErrorKind::Internal(_) => "internal error rolling this agent (see server logs)".to_string(),
        other => other.to_string(),
    }
}

/// Result of rolling one tool. `total` is every running container agent found;
/// `skipped_busy` are working agents intentionally left alone (rolling a busy
/// agent would interrupt in-flight work and risk a redelivered assignment
/// double-executing against the fresh container). `succeeded + failed` cover the
/// idle agents actually rolled, one `RollAgentResult` each. An all-skipped /
/// empty roll is a successful no-op.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RollReport {
    pub tool: String,
    pub total: usize,
    pub succeeded: usize,
    pub failed: usize,
    pub skipped_busy: usize,
    pub results: Vec<RollAgentResult>,
}

impl RollReport {
    pub(crate) fn build(tool: &str, results: Vec<RollAgentResult>, skipped_busy: usize) -> Self {
        let succeeded = results.iter().filter(|r| r.ok).count();
        let failed = results.len() - succeeded;
        Self { tool: tool.to_string(), total: results.len() + skipped_busy, succeeded, failed, skipped_busy, results }
    }
}

/// 409 error for a concurrent roll of the same tool. The user-visible error
/// contract lives here in the domain (services must not own `ErrorKind` policy).
pub(crate) fn roll_in_progress_error(tool: &str) -> AppError {
    ErrorKind::Conflict(format!("a roll for '{tool}' is already in progress")).into()
}

/// 503 error when the container runtime is unavailable on this deployment. Surfaced
/// ONCE for the whole roll instead of as N identical per-agent "internal error"
/// lines, so an operator sees the real (environment-level) cause.
pub(crate) fn roll_runtime_unavailable_error() -> AppError {
    ErrorKind::Unavailable("the container runtime is unavailable on this server; no agents were rolled".to_string())
        .into()
}

/// `{ ok: true, data: <roll report> }` envelope.
pub(crate) fn cli_image_roll_response(report: RollReport) -> Value {
    json!({ "ok": true, "data": report })
}

// ---------------------------------------------------------------------------
// Operator-initiated local build (POST /admin/cli-images/claude/build)
// ---------------------------------------------------------------------------

/// Which tools may be built locally. Exactly the inverse boundary of
/// [`RollToolPolicy`]: only `claude` (no public registry image — its license
/// requires a self-build) is buildable; registry tools are pulled, never built.
/// Asserted in the route AND re-asserted in the service, like the roll path.
pub(crate) struct LocalBuildToolPolicy;

impl LocalBuildToolPolicy {
    /// Accept only the local-build tool. Registry tools and unknown values are
    /// rejected with 422 — NOT 404, since the route path matched; the value is
    /// just not a locally-buildable tool.
    pub(crate) fn ensure_local_buildable(tool: &str) -> AppResult<()> {
        if tool == agentforge_core::CliToolKind::Claude.as_str() {
            Ok(())
        } else {
            Err(ErrorKind::Unprocessable(format!(
                "'{tool}' is not a locally-built CLI tool; only 'claude' is built on this server \
                 (registry tools update via pull)"
            ))
            .into())
        }
    }
}

/// 409 error for a claude build that is already in flight (manual or
/// auto-build). The user-visible error contract lives here in the domain.
pub(crate) fn claude_build_in_progress_error() -> AppError {
    ErrorKind::Conflict("a claude image build is already in progress".to_string()).into()
}

/// 503 error when the container runtime is unavailable on this deployment —
/// the build cannot start at all. Mirrors the roll path's runtime-down error.
pub(crate) fn claude_build_runtime_unavailable_error() -> AppError {
    ErrorKind::Unavailable(
        "the container runtime is unavailable on this server; the image build cannot start".to_string(),
    )
    .into()
}

/// 503 error when the npm registry could not be reached / parsed, so the
/// target version is unknown and no build was started. Carries the operator
/// detail verbatim (the npm lookup error strings hold no secrets).
pub(crate) fn claude_version_lookup_failed_error(detail: &str) -> AppError {
    ErrorKind::Unavailable(format!(
        "could not determine the latest Claude Code version from the npm registry; no build was started: {detail}"
    ))
    .into()
}

/// `202 { ok: true, started: true, targetVersion }` body — the build was
/// accepted and runs in the background; progress lands in the status report.
pub(crate) fn cli_image_build_response(target_version: &str) -> Value {
    json!({ "ok": true, "started": true, "targetVersion": target_version })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roll_policy_rejects_claude_and_unknown() {
        assert!(RollToolPolicy::ensure_rollable("codex").is_ok());
        // claude has no public image and is never auto-managed → not rollable.
        assert!(RollToolPolicy::ensure_rollable("claude").is_err());
        assert!(RollToolPolicy::ensure_rollable("nonsense").is_err());
        assert!(RollToolPolicy::ensure_rollable("").is_err());
    }

    #[test]
    fn roll_report_counts_and_empty_is_noop() {
        let empty = RollReport::build("codex", vec![], 0);
        assert_eq!((empty.total, empty.succeeded, empty.failed, empty.skipped_busy), (0, 0, 0, 0));

        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let c = Uuid::new_v4();
        // 3 rolled (1 ok, 2 failed) + 3 skipped-busy → total 6.
        let report = RollReport::build(
            "codex",
            vec![
                RollAgentResult::respawned(a),
                RollAgentResult::failed_now_stopped(b, "boom".into()),
                RollAgentResult::failed_still_running(c, "stop failed".into()),
            ],
            3,
        );
        assert_eq!((report.total, report.succeeded, report.failed, report.skipped_busy), (6, 1, 2, 3));
        // The two failure modes carry the truthful post-condition: start-fail →
        // confirmed stopped; stop-fail → unconfirmed (stopped = false).
        let now_stopped = report.results.iter().find(|r| r.agent_id == b).unwrap();
        assert!(now_stopped.stopped && now_stopped.error.as_deref() == Some("boom"));
        let still_running = report.results.iter().find(|r| r.agent_id == c).unwrap();
        assert!(!still_running.stopped && still_running.error.as_deref() == Some("stop failed"));
    }

    #[test]
    fn client_safe_roll_error_redacts_internal_but_keeps_typed() {
        let internal: AppError = anyhow::anyhow!("docker socket: /var/run/docker.sock EACCES").into();
        assert_eq!(client_safe_roll_error(&internal), "internal error rolling this agent (see server logs)");
        // a typed domain error keeps its operator-facing message.
        let typed: AppError = ErrorKind::Conflict("agent is mid-restart".into()).into();
        assert!(client_safe_roll_error(&typed).contains("agent is mid-restart"));
    }

    fn state(s: &str) -> CliToolImageState {
        CliToolImageState {
            state: s.to_string(),
            update_mode: "registry".to_string(),
            local_digest: Some("sha256:local".into()),
            remote_digest: Some("sha256:remote".into()),
            last_checked_unix: Some(1_700_000_000),
            ..Default::default()
        }
    }

    #[test]
    fn unchecked_tool_is_pending_with_zero_agents() {
        let report = CliImageStatusReport::build(
            false,
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
        assert!(report.tools.iter().all(|t| !t.building));
        assert!(!report.auto_update_enabled);
        assert!(!report.claude_auto_build_enabled);
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
            false,
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
        assert_eq!(codex.update_mode, "registry");
        assert!(report.prune.enabled);
        assert_eq!(report.prune.removed, 3);

        // gemini has no snapshot or count → pending / 0, never dropped.
        let gemini = report.tools.iter().find(|t| t.tool == "gemini").unwrap();
        assert_eq!(gemini.state, "pending");
        assert_eq!(gemini.agents_with_container, 0);
    }

    // ------------------------------------------------------------------
    // claude local build
    // ------------------------------------------------------------------

    #[test]
    fn claude_row_reports_local_build_mode_even_when_pending() {
        // claude in the report driver list but no recorded state → a pending
        // row that is STILL labeled local_build (mode comes from the canonical
        // per-tool policy, not the snapshot).
        let report = CliImageStatusReport::build(
            false,
            true,
            900,
            "ghcr.io/x".into(),
            "latest".into(),
            &["claude", "codex"],
            &BTreeMap::new(),
            &BTreeMap::new(),
            false,
            CliImagePruneSummary::default(),
        );
        assert!(report.claude_auto_build_enabled);
        let claude = report.tools.iter().find(|t| t.tool == "claude").unwrap();
        assert_eq!(claude.state, "pending");
        assert_eq!(claude.update_mode, "local_build");
        assert!(claude.local_version.is_none() && claude.remote_version.is_none());
        let codex = report.tools.iter().find(|t| t.tool == "codex").unwrap();
        assert_eq!(codex.update_mode, "registry");
    }

    #[test]
    fn tool_status_serializes_camel_case_with_versions_and_building() {
        let recorded = CliToolImageState {
            state: "update_available".to_string(),
            update_mode: "local_build".to_string(),
            local_version: Some("2.1.100".into()),
            remote_version: Some("2.1.173".into()),
            building: true,
            last_checked_unix: Some(1_700_000_000),
            ..Default::default()
        };
        let mut snap = BTreeMap::new();
        snap.insert("claude".to_string(), recorded);

        let report = CliImageStatusReport::build(
            true,
            true,
            900,
            "ghcr.io/x".into(),
            "latest".into(),
            &["claude"],
            &snap,
            &BTreeMap::new(),
            false,
            CliImagePruneSummary::default(),
        );
        let value = serde_json::to_value(&report).unwrap();

        // top-level report contract: camelCase, claude flag present.
        assert_eq!(value["claudeAutoBuildEnabled"], true);
        assert!(value.get("claude_auto_build_enabled").is_none());

        let claude = &value["tools"][0];
        assert_eq!(claude["tool"], "claude");
        assert_eq!(claude["state"], "update_available");
        assert_eq!(claude["updateMode"], "local_build");
        assert_eq!(claude["localVersion"], "2.1.100");
        assert_eq!(claude["remoteVersion"], "2.1.173");
        assert_eq!(claude["building"], true);
        assert_eq!(claude["localDigest"], serde_json::Value::Null);
        // snake_case keys must not leak.
        assert!(claude.get("update_mode").is_none());
        assert!(claude.get("local_version").is_none());
        assert!(claude.get("remote_version").is_none());
    }

    #[test]
    fn local_build_policy_accepts_only_claude() {
        assert!(LocalBuildToolPolicy::ensure_local_buildable("claude").is_ok());
        // registry tools are pulled, never built — and unknown values fail too.
        for tool in ["codex", "gemini", "opencode", "nonsense", ""] {
            let err = LocalBuildToolPolicy::ensure_local_buildable(tool).expect_err("must reject");
            assert!(
                matches!(err.kind, ErrorKind::Unprocessable(ref msg) if msg.contains("locally-built")),
                "expected 422 Unprocessable, got {err:?}"
            );
        }
    }

    #[test]
    fn claude_build_error_constructors_carry_http_contract() {
        assert!(matches!(claude_build_in_progress_error().kind, ErrorKind::Conflict(_)));
        assert!(matches!(claude_build_runtime_unavailable_error().kind, ErrorKind::Unavailable(_)));
        let lookup = claude_version_lookup_failed_error("npm registry returned HTTP 503");
        assert!(
            matches!(lookup.kind, ErrorKind::Unavailable(ref msg) if msg.contains("npm registry returned HTTP 503")
                && msg.contains("no build was started"))
        );
    }

    #[test]
    fn build_response_reports_started_and_target_version() {
        let response = cli_image_build_response("2.1.173");
        assert_eq!(response["ok"], true);
        assert_eq!(response["started"], true);
        assert_eq!(response["targetVersion"], "2.1.173");
    }
}

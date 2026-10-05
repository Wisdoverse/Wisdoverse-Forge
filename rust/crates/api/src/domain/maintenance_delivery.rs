//! Revision-bound review evidence and explicit human outcomes. No I/O.

use agentforge_core::{AppError, AppResult, ErrorKind};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::maintenance::MaintenanceTrace;
use super::orchestration::TaskRunImageSummary;

pub(crate) fn encode_delivery<T: Serialize>(value: &T) -> AppResult<serde_json::Value> {
    serde_json::to_value(value).map_err(anyhow::Error::from).map_err(Into::into)
}
pub(crate) fn decode_delivery<T: serde::de::DeserializeOwned>(value: serde_json::Value) -> AppResult<T> {
    serde_json::from_value(value).map_err(anyhow::Error::from).map_err(Into::into)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ReportedCheckStatus {
    Passed,
    Failed,
    NotRun,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ReportedCheck {
    pub name: String,
    pub command: String,
    pub status: ReportedCheckStatus,
    pub evidence: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct VerificationInput {
    pub request_key: Uuid,
    pub expected_version: i64,
    pub expected_revision: String,
    pub run_id: Option<Uuid>,
    pub criteria: String,
    pub scope: String,
    pub environment_notes: String,
    pub cli_version: Option<String>,
    pub change_summary: String,
    pub checks: Vec<ReportedCheck>,
    pub unverified: Vec<String>,
    pub no_artifact_reason: Option<String>,
    pub comparison_key: Option<String>,
}

impl VerificationInput {
    pub(crate) fn prepare(mut self) -> AppResult<Self> {
        validate_identity(self.request_key, self.expected_version, &self.expected_revision)?;
        if self.run_id.is_some_and(|id| id.is_nil()) || self.checks.len() > 20 || self.unverified.len() > 20 {
            return Err(delivery_invalid());
        }
        self.criteria = bounded_text(self.criteria, 4000)?;
        self.scope = bounded_text(self.scope, 4000)?;
        self.environment_notes = bounded_text(self.environment_notes, 4000)?;
        self.cli_version = self.cli_version.map(|s| bounded_text(s, 256)).transpose()?;
        self.change_summary = bounded_text(self.change_summary, 8000)?;
        for check in &mut self.checks {
            check.name = bounded_text(std::mem::take(&mut check.name), 256)?;
            check.command = bounded_text(std::mem::take(&mut check.command), 1000)?;
            check.evidence = bounded_text(std::mem::take(&mut check.evidence), 2000)?;
        }
        for item in &mut self.unverified {
            *item = bounded_text(std::mem::take(item), 1000)?;
        }
        self.no_artifact_reason = self.no_artifact_reason.map(|s| bounded_text(s, 2000)).transpose()?;
        self.comparison_key = self.comparison_key.map(|key| key.trim().to_ascii_lowercase());
        if self.comparison_key.as_ref().is_some_and(|key| {
            key.len() > 64
                || !key.as_bytes().first().is_some_and(u8::is_ascii_alphanumeric)
                || !key.bytes().all(|c| c.is_ascii_alphanumeric() || b"._:-".contains(&c))
        }) {
            return Err(delivery_invalid());
        }
        Ok(self)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum HumanVerdict {
    Accepted,
    Rejected,
    Rework,
    Reopened,
}

impl HumanVerdict {
    pub(crate) fn as_str(&self) -> &'static str {
        match self {
            Self::Accepted => "accepted",
            Self::Rejected => "rejected",
            Self::Rework => "rework",
            Self::Reopened => "reopened",
        }
    }
}

/// Cumulative task totals. An omitted category remains unknown, including zero.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct HumanMinutes {
    pub setup: Option<i32>,
    pub handling: Option<i32>,
    pub review: Option<i32>,
    pub recovery: Option<i32>,
    pub rework: Option<i32>,
    pub operation: Option<i32>,
}

impl HumanMinutes {
    pub(crate) fn values(&self) -> [Option<i32>; 6] {
        [self.setup, self.handling, self.review, self.recovery, self.rework, self.operation]
    }

    pub(crate) fn total(&self) -> Option<i64> {
        self.values().into_iter().try_fold(0_i64, |sum, value| value.map(|v| sum + i64::from(v)))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct DecisionInput {
    pub request_key: Uuid,
    pub report_id: Uuid,
    pub expected_version: i64,
    pub expected_revision: String,
    pub verdict: HumanVerdict,
    pub reason: String,
    pub human_minutes: HumanMinutes,
    pub baseline_minutes: Option<i32>,
}

impl DecisionInput {
    pub(crate) fn prepare(mut self) -> AppResult<Self> {
        validate_identity(self.request_key, self.expected_version, &self.expected_revision)?;
        self.reason = bounded_text(self.reason, 4000)?;
        if self.report_id.is_nil()
            || self.human_minutes.values().into_iter().flatten().any(|v| !(0..=100_000).contains(&v))
            || self.baseline_minutes.is_some_and(|v| !(0..=600_000).contains(&v))
        {
            return Err(delivery_invalid());
        }
        Ok(self)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct HandoffInput {
    pub request_key: Uuid,
    pub expected_version: i64,
    pub expected_revision: String,
    pub reason: String,
    pub next_step: String,
}

impl HandoffInput {
    pub(crate) fn prepare(mut self) -> AppResult<Self> {
        validate_identity(self.request_key, self.expected_version, &self.expected_revision)?;
        self.reason = bounded_text(self.reason, 4000)?;
        self.next_step = bounded_text(self.next_step, 4000)?;
        Ok(self)
    }
}

fn validate_identity(key: Uuid, version: i64, revision: &str) -> AppResult<()> {
    if key.is_nil() || version < 0 || !valid_revision(revision) {
        return Err(delivery_invalid());
    }
    Ok(())
}

pub(crate) fn valid_revision(revision: &str) -> bool {
    matches!(revision.len(), 40 | 64) && revision.bytes().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
}

fn bounded_text(value: String, max: usize) -> AppResult<String> {
    let value = value.trim().to_owned();
    if value.is_empty() || value.len() > max || value.contains('\0') {
        return Err(delivery_invalid());
    }
    Ok(value)
}

pub(crate) fn delivery_invalid() -> AppError {
    ErrorKind::ValidationWithCode {
        code: "errors.maintenance.delivery_invalid",
        message: "Provide the current task version, revision and the requested review details; keep text and minutes within the displayed limits.".into(),
    }.into()
}

pub(crate) fn delivery_conflict() -> AppError {
    ErrorKind::Conflict("This task, run or revision changed. Refresh its report before recording new evidence. Reuse a submission reference only for the same unchanged submission.".into()).into()
}

pub(crate) fn delivery_unavailable() -> AppError {
    ErrorKind::ValidationWithCode {
        code: "errors.maintenance.delivery_unavailable",
        message: "The current produced revision could not be verified. Refresh after GitHub recovers or review the changed revision before recording acceptance.".into(),
    }.into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ObservedCheck {
    pub kind: String,
    pub name: String,
    pub state: String,
    pub conclusion: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GithubVerification {
    pub status: String,
    pub checked_at: Option<String>,
    pub revision: Option<String>,
    pub complete: bool,
    pub checks: Vec<ObservedCheck>,
}

impl GithubVerification {
    pub(crate) fn absent(status: &str) -> Self {
        Self { status: status.into(), checked_at: None, revision: None, complete: false, checks: vec![] }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RuntimeEvidence {
    pub run_id: Uuid,
    pub agent_id: Uuid,
    pub state: String,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub runtime_kind: Option<String>,
    pub cli_tool: Option<String>,
    pub provider_name: Option<String>,
    pub image: Option<RuntimeImageEvidence>,
}

/// Explicit nulls keep the report contract stable when image metadata is partial.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RuntimeImageEvidence {
    pub source: String,
    pub image_id: String,
    pub manifest_digest: Option<String>,
    pub version: Option<String>,
    pub version_source: String,
    pub trust: Option<String>,
}
impl From<TaskRunImageSummary> for RuntimeImageEvidence {
    fn from(image: TaskRunImageSummary) -> Self {
        Self {
            source: image.source,
            image_id: image.image_id,
            manifest_digest: image.manifest_digest,
            version: image.version,
            version_source: image.version_source,
            trust: image.trust,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct EvidenceReference {
    pub source_type: String,
    pub source_id: Uuid,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ReportSnapshot {
    pub title: String,
    pub brief: Option<String>,
    pub repository: String,
    pub revision_kind: String,
    pub scope: String,
    pub environment_notes: String,
    pub cli_version: Option<String>,
    pub change_summary: String,
    pub reported_checks: Vec<ReportedCheck>,
    pub unverified: Vec<String>,
    pub no_artifact_reason: Option<String>,
    pub runtime: Option<RuntimeEvidence>,
    pub evidence: Vec<EvidenceReference>,
    pub evidence_complete: bool,
    pub github: GithubVerification,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct VerificationReport {
    pub id: Uuid,
    pub task_id: Uuid,
    pub run_id: Option<Uuid>,
    pub author_id: Uuid,
    pub task_version: i64,
    pub revision: String,
    pub starting_revision: String,
    pub criteria: String,
    pub comparison_key: Option<String>,
    pub created_at: String,
    pub snapshot: ReportSnapshot,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ReviewDecision {
    pub id: Uuid,
    pub report_id: Uuid,
    pub reviewer_id: Uuid,
    pub verdict: HumanVerdict,
    pub reason: String,
    pub human_minutes: HumanMinutes,
    pub total_minutes: Option<i64>,
    pub github: Option<GithubVerification>,
    pub baseline_minutes: Option<i32>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BridgeRetry {
    pub state: String,
    pub attempts: i32,
    pub limit: i32,
    pub next_attempt_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RecoverySnapshot {
    pub observed_at: String,
    pub task_state: String,
    pub attempt: i32,
    pub failure_code: Option<String>,
    pub blocked_reason: Option<String>,
    pub manual_retry_allowed: bool,
    pub merge_attempts: i32,
    pub merge_limit: i32,
    pub review_status: Option<String>,
    pub bridge: Option<BridgeRetry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct HandoffSnapshot {
    pub observed_at: String,
    pub revision: String,
    pub run_id: Option<Uuid>,
    pub task_version: i64,
    pub recovery: RecoverySnapshot,
    pub report_ids: Vec<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MaintenanceHandoff {
    pub id: Uuid,
    pub task_id: Uuid,
    pub author_id: Uuid,
    pub reason: String,
    pub next_step: String,
    pub created_at: String,
    pub snapshot: HandoffSnapshot,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MaintenanceDelivery {
    pub trace: MaintenanceTrace,
    pub task_version: i64,
    pub current_revision: String,
    pub latest_run_id: Option<Uuid>,
    pub recovery: RecoverySnapshot,
    pub current_checks: GithubVerification,
    pub reports: Vec<VerificationReport>,
    pub decisions: Vec<ReviewDecision>,
    pub handoffs: Vec<MaintenanceHandoff>,
    pub has_more_reports: bool,
    pub has_more_decisions: bool,
    pub has_more_handoffs: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct OutcomeQuery {
    pub from: Option<chrono::DateTime<chrono::Utc>>,
    pub to: Option<chrono::DateTime<chrono::Utc>>,
    pub project_id: Option<Uuid>,
    pub cursor: Option<Uuid>,
}

pub(crate) struct OutcomeWindow {
    pub from: chrono::DateTime<chrono::Utc>,
    pub to: chrono::DateTime<chrono::Utc>,
    pub project_id: Option<Uuid>,
    pub cursor: Option<Uuid>,
}

impl OutcomeQuery {
    pub(crate) fn prepare(self, now: chrono::DateTime<chrono::Utc>) -> AppResult<OutcomeWindow> {
        let to = self.to.unwrap_or(now);
        let from = self.from.unwrap_or(to - chrono::Duration::days(7));
        if from >= to
            || to - from > chrono::Duration::days(120)
            || to > now + chrono::Duration::minutes(1)
            || self.project_id.is_some_and(|id| id.is_nil())
            || self.cursor.is_some_and(|id| id.is_nil())
        {
            return Err(delivery_invalid());
        }
        Ok(OutcomeWindow { from, to, project_id: self.project_id, cursor: self.cursor })
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct OutcomeSummary {
    pub submitted: i64,
    pub accepted: i64,
    pub rejected: i64,
    pub rework: i64,
    pub reopened: i64,
    pub awaiting_review: i64,
    pub stale_reviews: i64,
    pub failed: i64,
    pub canceled: i64,
    pub complete_effort_tasks: i64,
    pub human_minutes: Option<i64>,
    pub paired_baseline_tasks: i64,
    pub paired_human_minutes: Option<i64>,
    pub baseline_minutes: Option<i64>,
    pub accepted_in_period: i64,
    pub reopened_in_period: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct OutcomeTask {
    pub task_id: Uuid,
    pub title: String,
    pub state: String,
    pub created_at: String,
    pub report: Option<VerificationReport>,
    pub decision: Option<ReviewDecision>,
    pub review_current: bool,
    pub lead_time_minutes: Option<i64>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MaintenanceOutcomes {
    pub from: String,
    pub to: String,
    pub project_id: Option<Uuid>,
    pub observed_at: String,
    pub summary: OutcomeSummary,
    pub tasks: Vec<OutcomeTask>,
    pub next_cursor: Option<Uuid>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ComparisonQuery {
    pub report_ids: String,
}

impl ComparisonQuery {
    pub(crate) fn prepare(self) -> AppResult<Vec<Uuid>> {
        let ids: Vec<Uuid> =
            self.report_ids.split(',').map(str::parse).collect::<Result<_, _>>().map_err(|_| delivery_invalid())?;
        let unique: std::collections::HashSet<Uuid> = ids.iter().copied().collect();
        if !(2..=8).contains(&ids.len()) || unique.len() != ids.len() || ids.iter().any(Uuid::is_nil) {
            return Err(delivery_invalid());
        }
        Ok(ids)
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ComparisonRow {
    pub report: VerificationReport,
    pub decision: Option<ReviewDecision>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MaintenanceComparison {
    pub conditions_match: bool,
    pub distinct_clis: usize,
    pub reasons: Vec<String>,
    pub rows: Vec<ComparisonRow>,
}

pub(crate) fn compare_reports(rows: Vec<ComparisonRow>) -> MaintenanceComparison {
    let mut reasons = vec![];
    let clis: std::collections::HashSet<&str> = rows
        .iter()
        .filter_map(|r| r.report.snapshot.runtime.as_ref().and_then(|run| run.cli_tool.as_deref()))
        .collect();
    let runs: std::collections::HashSet<Uuid> = rows.iter().filter_map(|r| r.report.run_id).collect();
    if let Some(first) = rows.first() {
        if rows.iter().any(|r| {
            r.report.snapshot.repository != first.report.snapshot.repository
                || r.report.starting_revision != first.report.starting_revision
        }) {
            reasons.push("Repository or starting revision differs.".into());
        }
        if rows.iter().any(|r| {
            r.report.criteria != first.report.criteria
                || r.report.snapshot.title != first.report.snapshot.title
                || r.report.snapshot.scope != first.report.snapshot.scope
                || r.report.snapshot.brief != first.report.snapshot.brief
                || r.report.snapshot.environment_notes != first.report.snapshot.environment_notes
        }) {
            reasons.push("Task instructions, scope, acceptance criteria or recorded environment differs.".into());
        }
        if first.report.comparison_key.is_none()
            || rows.iter().any(|r| r.report.comparison_key != first.report.comparison_key)
        {
            reasons.push("Reports need the same explicit comparison reference.".into());
        }
    }
    if runs.len() != rows.len()
        || rows.iter().any(|r| {
            r.report.snapshot.runtime.as_ref().is_none_or(|run| {
                run.runtime_kind.as_deref() != Some("container")
                    || run.finished_at.is_none()
                    || !matches!(run.cli_tool.as_deref(), Some("claude" | "codex" | "gemini" | "opencode"))
            }) || r.report.snapshot.cli_version.is_none()
        })
    {
        reasons.push("Distinct finished container executions and reported CLI versions are required.".into());
    }
    if clis.len() < 2 {
        reasons.push("At least two different supported Container CLIs are required.".into());
    }
    MaintenanceComparison { conditions_match: reasons.is_empty(), distinct_clis: clis.len(), reasons, rows }
}

#[cfg(test)]
mod tests;

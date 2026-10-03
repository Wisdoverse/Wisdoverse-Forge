//! Snapshot adapters and bounded read paths for revision-bound review evidence.

use agentforge_core::{AppResult, TenantScope};
use agentforge_db::entities::{OrchestrationTask, TaskRun};
use uuid::Uuid;

use super::SelfFixService;
use crate::domain::maintenance_delivery::*;
use crate::domain::orchestration::TaskLifecyclePolicy;
use crate::repositories::orchestration::maintenance_delivery::{DecisionRow, HandoffRow, ReportRow};
use crate::repositories::orchestration::maintenance_request::MaintenanceRequestRow;

pub(super) use crate::domain::maintenance_delivery::{decode_delivery as decode, encode_delivery as json};
pub(super) fn report(row: ReportRow) -> AppResult<VerificationReport> {
    Ok(VerificationReport {
        id: row.id,
        task_id: row.task_id,
        run_id: row.run_id,
        author_id: row.author_id,
        task_version: row.task_version,
        revision: row.revision,
        starting_revision: row.starting_revision,
        criteria: row.criteria,
        comparison_key: row.comparison_key,
        created_at: row.created_at.to_rfc3339(),
        snapshot: decode(row.snapshot)?,
    })
}
pub(super) fn decision(row: DecisionRow) -> AppResult<ReviewDecision> {
    let input: DecisionInput = decode(row.input)?;
    Ok(ReviewDecision {
        id: row.id,
        report_id: row.report_id,
        reviewer_id: row.reviewer_id,
        total_minutes: input.human_minutes.total(),
        verdict: input.verdict,
        reason: input.reason,
        human_minutes: input.human_minutes,
        baseline_minutes: input.baseline_minutes,
        github: row.github_observation.map(decode).transpose()?,
        created_at: row.created_at.to_rfc3339(),
    })
}
pub(super) fn handoff(row: HandoffRow) -> AppResult<MaintenanceHandoff> {
    Ok(MaintenanceHandoff {
        id: row.id,
        task_id: row.task_id,
        author_id: row.author_id,
        reason: row.reason,
        next_step: row.next_step,
        created_at: row.created_at.to_rfc3339(),
        snapshot: decode(row.snapshot)?,
    })
}
pub(super) fn revision<'a>(task: &'a OrchestrationTask, source: &'a MaintenanceRequestRow) -> &'a str {
    task.pr_head_sha.as_deref().or(task.base_commit_sha.as_deref()).unwrap_or(&source.starting_sha)
}
pub(super) fn ensure_current(
    task: &OrchestrationTask,
    source: &MaintenanceRequestRow,
    version: i64,
    expected: &str,
    run: Option<&TaskRun>,
    run_id: Option<Uuid>,
) -> AppResult<()> {
    if task.row_version != version || revision(task, source) != expected || run.map(|r| r.id) != run_id {
        return Err(delivery_conflict());
    }
    Ok(())
}

impl SelfFixService {
    pub(super) async fn delivery_source(&self, scope: &TenantScope, task: Uuid) -> AppResult<MaintenanceRequestRow> {
        self.maintenance_requests.find_by_task(scope, task).await?.ok_or_else(delivery_invalid)
    }

    pub(super) async fn observe_checks(
        &self,
        source: &MaintenanceRequestRow,
        task: &OrchestrationTask,
    ) -> GithubVerification {
        let Some(number) = task.pr_number else {
            return GithubVerification::absent("not_created");
        };
        let observed =
            match self.github.as_deref().filter(|g| g.repository_slug().eq_ignore_ascii_case(&source.repository)) {
                Some(github) => tokio::time::timeout(
                    std::time::Duration::from_secs(25),
                    github.verification_checks(number, revision(task, source)),
                )
                .await
                .ok()
                .and_then(Result::ok),
                None => None,
            };
        observed.unwrap_or_else(|| GithubVerification {
            checked_at: Some(chrono::Utc::now().to_rfc3339()),
            ..GithubVerification::absent("unavailable")
        })
    }

    pub(super) async fn recovery(&self, scope: &TenantScope, task: &OrchestrationTask) -> AppResult<RecoverySnapshot> {
        let bridge = self.delivery.bridge(scope, task.id).await?.map(|b| BridgeRetry {
            state: b.status.clone(),
            attempts: b.attempts,
            limit: b.max_attempts,
            next_attempt_at: (b.status == "pending").then(|| b.run_at.to_rfc3339()),
        });
        Ok(RecoverySnapshot {
            observed_at: chrono::Utc::now().to_rfc3339(),
            task_state: task.status.clone(),
            attempt: task.attempt,
            failure_code: task.failure_code.clone(),
            blocked_reason: task.blocked_reason.clone(),
            manual_retry_allowed: TaskLifecyclePolicy::ensure_can_retry(
                &task.status,
                task.blocked_reason.as_deref(),
                task.requires_approval,
                task.retryable,
            )
            .is_ok(),
            merge_attempts: task.merge_attempts,
            merge_limit: self.max_merge_attempts,
            review_status: task.review_status.clone(),
            bridge,
        })
    }

    pub(crate) async fn maintenance_delivery(
        &self,
        scope: &TenantScope,
        task_id: Uuid,
    ) -> AppResult<Option<MaintenanceDelivery>> {
        self.require_platform_admin(scope).await?;
        let task = self.tasks.find_by_id(scope, task_id).await?;
        let Some(source) = self.maintenance_requests.find_by_task(scope, task_id).await? else {
            return Ok(None);
        };
        let run = self.delivery.latest_run(scope, task_id).await?;
        let (trace, checks) = tokio::join!(self.maintenance_trace(scope, task_id), self.observe_checks(&source, &task));
        let trace = trace?.ok_or_else(delivery_invalid)?;
        let mut reports = self.delivery.reports(scope, task_id).await?;
        let mut decisions = self.delivery.decisions(scope, task_id).await?;
        let mut handoffs = self.delivery.handoffs(scope, task_id).await?;
        let (more_reports, more_decisions, more_handoffs) =
            (reports.len() > 20, decisions.len() > 100, handoffs.len() > 20);
        reports.truncate(20);
        decisions.truncate(100);
        handoffs.truncate(20);
        let recovery = self.recovery(scope, &task).await?;
        let fresh = self.tasks.find_by_id(scope, task_id).await?;
        let fresh_run = self.delivery.latest_run(scope, task_id).await?;
        ensure_current(
            &fresh,
            &source,
            task.row_version,
            revision(&task, &source),
            fresh_run.as_ref(),
            run.as_ref().map(|r| r.id),
        )?;
        self.require_platform_admin(scope).await?;
        Ok(Some(MaintenanceDelivery {
            trace,
            task_version: task.row_version,
            current_revision: revision(&task, &source).into(),
            latest_run_id: run.map(|r| r.id),
            recovery,
            current_checks: checks,
            reports: reports.into_iter().map(report).collect::<AppResult<_>>()?,
            decisions: decisions.into_iter().map(decision).collect::<AppResult<_>>()?,
            handoffs: handoffs.into_iter().map(handoff).collect::<AppResult<_>>()?,
            has_more_reports: more_reports,
            has_more_decisions: more_decisions,
            has_more_handoffs: more_handoffs,
        }))
    }

    pub(crate) async fn maintenance_comparison(
        &self,
        scope: &TenantScope,
        query: ComparisonQuery,
    ) -> AppResult<MaintenanceComparison> {
        self.require_platform_admin(scope).await?;
        let mut rows = vec![];
        for id in query.prepare()? {
            let row = self.delivery.report(scope, id).await?.ok_or_else(delivery_invalid)?;
            let verdict = self.delivery.latest_decision(scope, id).await?.map(decision).transpose()?;
            rows.push(ComparisonRow { report: report(row)?, decision: verdict });
        }
        self.require_platform_admin(scope).await?;
        Ok(compare_reports(rows))
    }

    pub(crate) async fn maintenance_report(&self, scope: &TenantScope, id: Uuid) -> AppResult<ComparisonRow> {
        self.require_platform_admin(scope).await?;
        let saved = self.delivery.report(scope, id).await?.ok_or_else(delivery_invalid)?;
        let verdict = self.delivery.latest_decision(scope, id).await?.map(decision).transpose()?;
        self.require_platform_admin(scope).await?;
        Ok(ComparisonRow { report: report(saved)?, decision: verdict })
    }
}

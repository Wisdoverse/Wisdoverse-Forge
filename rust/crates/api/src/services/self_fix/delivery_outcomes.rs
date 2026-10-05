use super::SelfFixService;
use super::delivery::{decision, decode, report};
use crate::domain::maintenance_delivery::{MaintenanceOutcomes, OutcomeQuery, OutcomeSummary, OutcomeTask};
use crate::repositories::orchestration::maintenance_delivery::{
    DecisionRow, MaintenanceDeliveryRepository as Repo, ReportRow, SummaryRow,
};
use agentforge_core::{AppResult, TenantScope};

impl From<SummaryRow> for OutcomeSummary {
    fn from(r: SummaryRow) -> Self {
        Self {
            submitted: r.submitted,
            accepted: r.accepted,
            rejected: r.rejected,
            rework: r.rework,
            reopened: r.reopened,
            awaiting_review: r.awaiting_review,
            stale_reviews: r.stale_reviews,
            failed: r.failed,
            canceled: r.canceled,
            complete_effort_tasks: r.complete_effort_tasks,
            human_minutes: r.human_minutes,
            paired_baseline_tasks: r.paired_baseline_tasks,
            paired_human_minutes: r.paired_human_minutes,
            baseline_minutes: r.baseline_minutes,
            accepted_in_period: r.accepted_in_period,
            reopened_in_period: r.reopened_in_period,
        }
    }
}

impl SelfFixService {
    pub(crate) async fn maintenance_outcomes(
        &self,
        scope: &TenantScope,
        query: OutcomeQuery,
    ) -> AppResult<MaintenanceOutcomes> {
        self.require_platform_admin(scope).await?;
        let window = query.prepare(chrono::Utc::now())?;
        let mut tx = self.tasks.pool().begin().await?;
        Repo::read_snapshot_in_tx(&mut tx).await?;
        let summary = Repo::outcome_summary_in_tx(&mut tx, scope, &window).await?;
        let mut rows = Repo::outcome_rows_in_tx(&mut tx, scope, &window).await?;
        tx.commit().await?;
        let more = rows.len() > 100;
        rows.truncate(100);
        let cursor = if more { rows.last().map(|r| r.task_id) } else { None };
        let tasks = rows
            .into_iter()
            .map(|r| {
                Ok(OutcomeTask {
                    task_id: r.task_id,
                    title: r.title,
                    state: r.state,
                    created_at: r.created_at.to_rfc3339(),
                    report: r.report_data.map(decode::<ReportRow>).transpose()?.map(report).transpose()?,
                    decision: r.decision_data.map(decode::<DecisionRow>).transpose()?.map(decision).transpose()?,
                    review_current: r.review_current,
                    lead_time_minutes: r.lead_time_minutes,
                })
            })
            .collect::<AppResult<Vec<_>>>()?;
        self.require_platform_admin(scope).await?;
        Ok(MaintenanceOutcomes {
            from: window.from.to_rfc3339(),
            to: window.to.to_rfc3339(),
            project_id: window.project_id,
            observed_at: chrono::Utc::now().to_rfc3339(),
            summary: summary.into(),
            tasks,
            next_cursor: cursor,
        })
    }
}

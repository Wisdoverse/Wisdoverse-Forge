//! Append-only writes: authority and task locks are held through commit.

use agentforge_core::{AppResult, TenantScope};
use agentforge_db::entities::OrchestrationTask;
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

use super::SelfFixService;
use super::delivery::{decision, decode, ensure_current, handoff, json, report, revision};
use crate::domain::admin::AdminRolePolicy;
use crate::domain::maintenance_delivery::*;
use crate::repositories::orchestration::OrchestrationTaskRepository;
use crate::repositories::orchestration::maintenance_delivery::{
    CreateDecisionRow, CreateHandoffRow, MaintenanceDeliveryRepository as Repo, ReportRow,
};
use crate::repositories::user::UserRepository;
use crate::services::orchestration::task_run_summary;

async fn lock_task(tx: &mut Transaction<'_, Postgres>, scope: &TenantScope, id: Uuid) -> AppResult<OrchestrationTask> {
    let admin = UserRepository::find_is_admin_by_id_in_tx(tx, scope.user_id()).await?;
    AdminRolePolicy::require_platform_admin(admin)?;
    OrchestrationTaskRepository::lock_by_id_in_tx(tx, scope, id).await
}

impl SelfFixService {
    pub(crate) async fn create_verification(
        &self,
        scope: &TenantScope,
        id: Uuid,
        input: VerificationInput,
    ) -> AppResult<VerificationReport> {
        self.require_platform_admin(scope).await?;
        let input = input.prepare()?;
        let encoded = json(&input)?;
        let task = self.tasks.find_by_id(scope, id).await?;
        let source = self.delivery_source(scope, id).await?;
        let run = self.delivery.latest_run(scope, id).await?;
        let checks = self.observe_checks(&source, &task).await;
        let mut evidence = match &run {
            Some(run) => self.delivery.evidence(scope, run.id).await?,
            None => vec![],
        };
        let complete = evidence.len() <= 100;
        evidence.truncate(100);
        let mut tx = self.tasks.pool().begin().await?;
        let current = lock_task(&mut tx, scope, id).await?;
        if let Some(existing) = Repo::report_by_key_in_tx(&mut tx, scope, id, input.request_key).await? {
            if existing.input != encoded {
                return Err(delivery_conflict());
            }
            tx.commit().await?;
            return report(existing);
        }
        let current_run = Repo::latest_run_in_tx(&mut tx, scope, id).await?;
        if current.row_version != task.row_version {
            return Err(delivery_conflict());
        }
        ensure_current(
            &current,
            &source,
            input.expected_version,
            &input.expected_revision,
            current_run.as_ref(),
            input.run_id,
        )?;
        if run.as_ref().map(|r| r.id) != input.run_id
            || run.as_ref().is_some_and(|r| r.finished_at.is_none())
            || (current.pr_head_sha.is_none() && input.no_artifact_reason.is_none())
        {
            return Err(delivery_invalid());
        }
        let runtime = run.map(task_run_summary).map(|r| RuntimeEvidence {
            run_id: r.id,
            agent_id: r.agent_id,
            state: r.status,
            started_at: r.started_at,
            finished_at: r.finished_at,
            runtime_kind: r.runtime_kind,
            cli_tool: r.cli_tool,
            provider_name: r.provider_name,
            image: r.image.map(Into::into),
        });
        let snapshot = ReportSnapshot {
            title: current.title,
            brief: current.description,
            repository: source.repository,
            revision_kind: if current.pr_head_sha.is_some() { "produced" } else { "starting" }.into(),
            scope: input.scope.clone(),
            environment_notes: input.environment_notes.clone(),
            cli_version: input.cli_version.clone(),
            change_summary: input.change_summary.clone(),
            reported_checks: input.checks.clone(),
            unverified: input.unverified.clone(),
            no_artifact_reason: input.no_artifact_reason.clone(),
            runtime,
            evidence: evidence
                .into_iter()
                .map(|e| EvidenceReference {
                    source_type: e.source_type,
                    source_id: e.source_id,
                    created_at: e.created_at.to_rfc3339(),
                })
                .collect(),
            evidence_complete: complete,
            github: checks,
        };
        let row = ReportRow {
            id: Uuid::now_v7(),
            task_id: id,
            run_id: input.run_id,
            author_id: scope.user_id().as_uuid(),
            task_version: current.row_version,
            revision: input.expected_revision.clone(),
            starting_revision: current.base_commit_sha.unwrap_or(source.starting_sha),
            criteria: input.criteria.clone(),
            comparison_key: input.comparison_key.clone(),
            input: encoded,
            snapshot: json(&snapshot)?,
            created_at: chrono::Utc::now(),
        };
        let saved = Repo::insert_report_in_tx(&mut tx, scope, &row, input.request_key).await?;
        tx.commit().await?;
        report(saved)
    }

    pub(crate) async fn record_decision(
        &self,
        scope: &TenantScope,
        id: Uuid,
        input: DecisionInput,
    ) -> AppResult<ReviewDecision> {
        self.require_platform_admin(scope).await?;
        let input = input.prepare()?;
        let encoded = json(&input)?;
        let task = self.tasks.find_by_id(scope, id).await?;
        let source = self.delivery_source(scope, id).await?;
        let saved_report = self
            .delivery
            .report(scope, input.report_id)
            .await?
            .filter(|r| r.task_id == id)
            .ok_or_else(delivery_invalid)?;
        let checked = if input.verdict == HumanVerdict::Accepted {
            Some(self.observe_checks(&source, &task).await)
        } else {
            None
        };
        let mut tx = self.tasks.pool().begin().await?;
        let current = lock_task(&mut tx, scope, id).await?;
        if let Some(existing) = Repo::decision_by_key_in_tx(&mut tx, scope, id, input.request_key).await? {
            if existing.input != encoded {
                return Err(delivery_conflict());
            }
            tx.commit().await?;
            return decision(existing);
        }
        let run = Repo::latest_run_in_tx(&mut tx, scope, id).await?;
        if current.row_version != task.row_version {
            return Err(delivery_conflict());
        }
        ensure_current(
            &current,
            &source,
            input.expected_version,
            &input.expected_revision,
            run.as_ref(),
            saved_report.run_id,
        )?;
        let snapshot: ReportSnapshot = decode(saved_report.snapshot)?;
        if Repo::latest_report_in_tx(&mut tx, scope, id).await?.is_none_or(|r| r.id != input.report_id) {
            return Err(delivery_conflict());
        }
        if saved_report.revision != input.expected_revision
            || snapshot.title != current.title
            || snapshot.brief != current.description
        {
            return Err(delivery_conflict());
        }
        if input.verdict == HumanVerdict::Accepted
            && (current.pr_head_sha.is_none()
                || run.as_ref().is_none_or(|r| r.finished_at.is_none())
                || checked.as_ref().is_none_or(|c| {
                    c.status != "observed" || c.revision.as_deref() != Some(input.expected_revision.as_str())
                }))
        {
            return Err(delivery_unavailable());
        }
        let observation = checked.as_ref().map(json).transpose()?;
        let row = Repo::insert_decision_in_tx(
            &mut tx,
            scope,
            CreateDecisionRow {
                task: id,
                key: input.request_key,
                report: input.report_id,
                verdict: input.verdict.as_str(),
                reason: &input.reason,
                minutes: input.human_minutes.values(),
                baseline: input.baseline_minutes,
                input: &encoded,
                observation: observation.as_ref(),
            },
        )
        .await?;
        tx.commit().await?;
        decision(row)
    }

    pub(crate) async fn record_handoff(
        &self,
        scope: &TenantScope,
        id: Uuid,
        input: HandoffInput,
    ) -> AppResult<MaintenanceHandoff> {
        self.require_platform_admin(scope).await?;
        let input = input.prepare()?;
        let encoded = json(&input)?;
        let task = self.tasks.find_by_id(scope, id).await?;
        let source = self.delivery_source(scope, id).await?;
        let recovery = self.recovery(scope, &task).await?;
        let report_ids = self.delivery.reports(scope, id).await?.into_iter().take(20).map(|r| r.id).collect();
        let mut tx = self.tasks.pool().begin().await?;
        let current = lock_task(&mut tx, scope, id).await?;
        if let Some(existing) = Repo::handoff_by_key_in_tx(&mut tx, scope, id, input.request_key).await? {
            if existing.input != encoded {
                return Err(delivery_conflict());
            }
            tx.commit().await?;
            return handoff(existing);
        }
        let run = Repo::latest_run_in_tx(&mut tx, scope, id).await?;
        ensure_current(
            &current,
            &source,
            input.expected_version,
            &input.expected_revision,
            run.as_ref(),
            run.as_ref().map(|r| r.id),
        )?;
        if current.row_version != task.row_version || matches!(current.status.as_str(), "working" | "queued") {
            return Err(delivery_conflict());
        }
        let snapshot = HandoffSnapshot {
            observed_at: chrono::Utc::now().to_rfc3339(),
            revision: revision(&current, &source).into(),
            run_id: run.map(|r| r.id),
            task_version: current.row_version,
            recovery,
            report_ids,
        };
        let snapshot = json(&snapshot)?;
        let row = Repo::insert_handoff_in_tx(
            &mut tx,
            scope,
            CreateHandoffRow {
                task: id,
                key: input.request_key,
                reason: &input.reason,
                next_step: &input.next_step,
                input: &encoded,
                snapshot: &snapshot,
            },
        )
        .await?;
        tx.commit().await?;
        handoff(row)
    }
}

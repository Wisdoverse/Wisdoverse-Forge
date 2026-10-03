//! Deliberate intake and source/task/run/PR tracking. Never dispatches or merges.

use std::time::Duration;

use agentforge_core::{AppResult, TenantScope};
use chrono::Utc;
use uuid::Uuid;

use crate::domain::admin::AdminRolePolicy;
use crate::domain::maintenance::{
    MaintenanceExecution, MaintenanceObservation, MaintenanceRequestInput, MaintenanceSource, MaintenanceSubmission,
    MaintenanceTrace, destination_unavailable, ensure_repairable, maintenance_task_params, pull_request_url,
};
use crate::domain::self_fix::SelfFixPolicy;
use crate::repositories::identity::GroupRepository;
use crate::repositories::orchestration::{
    CreateMaintenanceRequestRow, CreateTaskRow, MaintenanceRequestRepository, OrchestrationTaskRepository,
};
use crate::repositories::user::UserRepository;
use crate::services::github_app::GithubAppClient;

use super::SelfFixService;

impl SelfFixService {
    pub(crate) async fn submit_maintenance_request(
        &self,
        scope: &TenantScope,
        input: MaintenanceRequestInput,
    ) -> AppResult<MaintenanceSubmission> {
        self.require_platform_admin(scope).await?;
        let request = input.prepare()?;
        let github = self.github.as_ref().ok_or_else(SelfFixPolicy::github_not_configured)?;
        let repository = github.repository_slug().to_ascii_lowercase();
        // An acknowledged source keeps its original task and snapshot, including
        // after completion or an upstream outage. No duplicate external reads.
        if let Some(existing) = self.maintenance_requests.find_by_source(scope, &repository, &request.source).await? {
            return Ok(MaintenanceSubmission { request_id: existing.id, task_id: existing.task_id, reused: true });
        }
        let setup = github.repository_setup().await?;
        let source = match request.source.pr_number {
            Some(number) => {
                let source = github.maintenance_pull_request(number).await?;
                ensure_repairable(&source, &setup.default_branch)?;
                Some(source)
            }
            None => None,
        };
        // The service owns cross-aggregate sequencing and the transaction boundary.
        let mut tx = self.tasks.pool().begin().await?;
        // Re-check after provider I/O and hold authority through commit.
        let admin = UserRepository::find_is_admin_by_id_in_tx(&mut tx, scope.user_id()).await?;
        AdminRolePolicy::require_platform_admin(admin)?;
        MaintenanceRequestRepository::lock_source_in_tx(&mut tx, scope, &repository, &request.source).await?;
        // Re-check under the source lock: concurrent submissions return one task.
        if let Some(existing) =
            MaintenanceRequestRepository::find_by_source_in_tx(&mut tx, scope, &repository, &request.source).await?
        {
            tx.commit().await?;
            return Ok(MaintenanceSubmission { request_id: existing.id, task_id: existing.task_id, reused: true });
        }
        if GroupRepository::lock_active_destination_in_tx(&mut tx, scope, request.group_id).await?.is_none() {
            return Err(destination_unavailable());
        }
        let params = maintenance_task_params(&request, &setup, source.as_ref());
        let task = OrchestrationTaskRepository::create_in_tx(
            &mut tx,
            scope,
            CreateTaskRow {
                group_id: Some(request.group_id),
                title: &request.title,
                description: Some(&request.brief),
                priority: "normal",
                params: Some(&params),
                initial_status: "backlog",
                self_fix: true,
                ..Default::default()
            },
        )
        .await?;
        let record = MaintenanceRequestRepository::insert_in_tx(
            &mut tx,
            scope,
            CreateMaintenanceRequestRow {
                task_id: task.id,
                repository: &repository,
                source_kind: request.source.kind,
                source_reference: &request.source.reference,
                source_pr_number: request.source.pr_number,
                source_head_sha: source.as_ref().map(|pr| pr.head_sha.as_str()),
                default_branch: &setup.default_branch,
                starting_sha: &setup.base_sha,
            },
        )
        .await?;
        tx.commit().await?;
        Ok(MaintenanceSubmission { request_id: record.id, task_id: record.task_id, reused: false })
    }

    pub(crate) async fn maintenance_trace(
        &self,
        scope: &TenantScope,
        task_id: Uuid,
    ) -> AppResult<Option<MaintenanceTrace>> {
        self.require_platform_admin(scope).await?;
        // Check tenant ownership before any provider read, including for old
        // self-fix tasks without source records.
        let task = self.tasks.find_by_id(scope, task_id).await?;
        let Some(record) = self.maintenance_requests.find_by_task(scope, task_id).await? else {
            return Ok(None);
        };
        let github =
            self.github.as_deref().filter(|client| client.repository_slug().eq_ignore_ascii_case(&record.repository));
        let (source_state, produced_state) = tokio::join!(
            observe(github, record.source_pr_number, record.source_head_sha.as_deref(), "not_applicable"),
            observe(github, task.pr_number, task.pr_head_sha.as_deref(), "not_created"),
        );
        let runs = self.runs.list_by_task(scope, task_id).await?;
        let executions = runs
            .into_iter()
            .map(|run| MaintenanceExecution {
                run_id: run.id,
                agent_id: run.agent_id.as_uuid(),
                state: run.status,
                started_at: run.started_at.to_rfc3339(),
                finished_at: run.finished_at.map(|time| time.to_rfc3339()),
            })
            .collect();
        // A permission change during slow provider reads must clear the result.
        self.require_platform_admin(scope).await?;
        Ok(Some(MaintenanceTrace {
            request_id: record.id,
            task_id,
            task_state: task.status,
            attempt: task.attempt,
            source: MaintenanceSource {
                kind: record.source_kind,
                reference: record.source_reference,
                url: record.source_pr_number.map(|number| pull_request_url(&record.repository, number)),
                submitted_head_sha: record.source_head_sha,
            },
            produced_pr_url: task.pr_number.map(|number| pull_request_url(&record.repository, number)),
            repository: record.repository,
            default_branch: record.default_branch,
            starting_sha: record.starting_sha,
            created_at: record.created_at.to_rfc3339(),
            executions,
            rebuild_base_sha: task.base_commit_sha,
            recorded_pr_head_sha: task.pr_head_sha,
            source_state,
            produced_state,
        }))
    }
}

async fn observe(
    github: Option<&GithubAppClient>,
    number: Option<i32>,
    recorded_head: Option<&str>,
    absent_status: &'static str,
) -> MaintenanceObservation {
    let Some(number) = number else {
        return MaintenanceObservation { status: absent_status, checked_at: None, snapshot: None, head_changed: false };
    };
    let snapshot = match github {
        Some(github) => tokio::time::timeout(Duration::from_secs(25), github.maintenance_pull_request(number))
            .await
            .ok()
            .and_then(Result::ok),
        None => None,
    };
    if snapshot.is_none() {
        tracing::warn!("Maintenance PR state could not be refreshed; reporting unavailable");
    }
    MaintenanceObservation {
        status: if snapshot.is_some() { "observed" } else { "unavailable" },
        checked_at: Some(Utc::now().to_rfc3339()),
        head_changed: snapshot.as_ref().is_some_and(|pr| recorded_head.is_some_and(|sha| sha != pr.head_sha)),
        snapshot,
    }
}

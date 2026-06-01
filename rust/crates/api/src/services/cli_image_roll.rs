//! Operator-initiated CLI agent-image roll (deployment-global, admin-gated).
//!
//! Drains + respawns the RUNNING container agents of ONE tool — across all orgs
//! — onto the freshly re-tagged `agentforge-agent:<tool>` image. UNLIKE the
//! auto-updater (which never touches running agents), this DOES interrupt live
//! work, so it is operator-initiated, never automatic, and never `claude`.
//!
//! Scope: each agent is rolled within its OWN persisted tenant scope
//! (org/user/workspace read from the row), reconstructed via `TenantScope`. This
//! is not privilege fabrication — it is the agent's real scope, reached only
//! through the admin gate, and the existing tenant-scoped `stop`/`start`
//! primitives enforce every per-org invariant. A roll = `stop` (removes the
//! container, clears `container_id`) then `start` (recreates from the resolved,
//! now-updated image). A failed `start` leaves that agent STOPPED; the per-agent
//! result records it so an operator can restart it via the normal control path.
//!
//! Safety: a `working` agent is SKIPPED (reported as `skipped_busy`) — rolling
//! one would interrupt its in-flight work and, because the sidecar dedup WAL is
//! container-local and destroyed with the container, risk a redelivered
//! assignment double-executing against the fresh sidecar. Only idle/offline
//! agents are rolled. This is a best-effort signal (agent `status` can lag), so
//! the feature still warrants a staging soak before prod-enable.
//!
//! Single-flight per tool: a `RollGuard` rejects a concurrent roll of the same
//! tool.

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use agentforge_core::{AgentId, AppConfig, AppResult, OrgId, TenantScope, UserId, WorkspaceId};
use agentforge_platform::DockerClient;
use sqlx::PgPool;

use crate::domain::context::ContextFeatureFlags;
use crate::repositories::admin::AdminRepository;
use crate::services::agent_container_control::AgentContainerControlService;
use crate::services::auth_callout::AuthCalloutService;

pub use crate::domain::cli_image::{RollAgentResult, RollReport};
pub(crate) use crate::domain::cli_image::{
    RollToolPolicy, cli_image_roll_response, client_safe_roll_error, roll_in_progress_error,
};

/// Single-flight guard: holds a tool name in the shared in-flight set for the
/// duration of a roll and removes it on drop (including on early return / panic).
struct RollGuard {
    inflight: Arc<Mutex<HashSet<String>>>,
    tool: String,
}

impl RollGuard {
    fn acquire(inflight: &Arc<Mutex<HashSet<String>>>, tool: &str) -> AppResult<Self> {
        let mut set = inflight.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        if !set.insert(tool.to_string()) {
            return Err(roll_in_progress_error(tool));
        }
        Ok(Self { inflight: Arc::clone(inflight), tool: tool.to_string() })
    }
}

impl Drop for RollGuard {
    fn drop(&mut self) {
        // Recover from a poisoned lock so a panic during a roll can never strand
        // the tool's slot (which would wedge all future rolls of that tool).
        let mut set = self.inflight.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        set.remove(&self.tool);
    }
}

pub struct CliImageRollService {
    repo: AdminRepository,
    control: AgentContainerControlService,
    inflight: Arc<Mutex<HashSet<String>>>,
}

impl CliImageRollService {
    pub(crate) fn from_runtime(
        pool: PgPool,
        config: &AppConfig,
        context_features: ContextFeatureFlags,
        encryption_key: Option<[u8; 32]>,
        docker: Option<Arc<DockerClient>>,
        auth_callout: Option<Arc<AuthCalloutService>>,
        inflight: Arc<Mutex<HashSet<String>>>,
    ) -> Self {
        Self {
            repo: AdminRepository::new(pool.clone()),
            control: AgentContainerControlService::from_runtime(
                pool,
                config,
                context_features,
                encryption_key,
                docker,
                auth_callout,
            ),
            inflight,
        }
    }

    /// Roll every running container agent of `tool`. Best-effort per agent — one
    /// agent's failure never aborts the rest; each outcome is collected.
    pub async fn roll(&self, tool: &str) -> AppResult<RollReport> {
        // Defense-in-depth: re-assert the allowlist here, never trusting only
        // the route (claude/unknown are rejected with 422).
        RollToolPolicy::ensure_rollable(tool)?;

        // Reject a concurrent roll of the same tool; the guard frees the slot on
        // drop regardless of how `roll` returns.
        let _guard = RollGuard::acquire(&self.inflight, tool)?;

        let targets = self.repo.running_container_agents_by_tool(tool).await?;
        let mut results = Vec::with_capacity(targets.len());
        let mut skipped_busy = 0usize;
        for target in targets {
            // Skip an agent with in-flight work — interrupting it risks a
            // redelivered assignment double-executing against the fresh sidecar
            // (its dedup WAL is destroyed with the container). Best-effort.
            if target.status == "working" {
                skipped_busy += 1;
                continue;
            }
            let scope = TenantScope::with_axes(
                OrgId::from(target.organization_id),
                UserId::from(target.user_id),
                target.workspace_id.map(WorkspaceId::from),
                None,
                None,
            );
            let agent_id = AgentId::from(target.id);
            let result = match self.roll_one(&scope, agent_id).await {
                Ok(()) => RollAgentResult::ok(target.id),
                Err(err) => {
                    // Full error to the server log; a client-safe message in the report.
                    tracing::warn!(agent_id = %target.id, tool, error = %err, "cli image roll: agent roll failed");
                    RollAgentResult::failed(target.id, client_safe_roll_error(&err))
                }
            };
            results.push(result);
        }

        let report = RollReport::build(tool, results, skipped_busy);
        tracing::info!(
            tool,
            total = report.total,
            succeeded = report.succeeded,
            failed = report.failed,
            skipped_busy = report.skipped_busy,
            "cli image roll complete"
        );
        Ok(report)
    }

    /// Stop (remove container + clear `container_id`) then start (recreate from
    /// the resolved image) one agent in its own scope.
    async fn roll_one(&self, scope: &TenantScope, agent_id: AgentId) -> AppResult<()> {
        self.control.stop(scope, agent_id).await?;
        self.control.start(scope, agent_id).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roll_guard_is_single_flight_per_tool() {
        let inflight = Arc::new(Mutex::new(HashSet::new()));
        let g1 = RollGuard::acquire(&inflight, "codex").expect("first acquire");
        // a second concurrent acquire for the same tool is rejected...
        assert!(RollGuard::acquire(&inflight, "codex").is_err());
        // ...but a different tool is fine.
        let _g2 = RollGuard::acquire(&inflight, "gemini").expect("other tool acquires");
        drop(g1);
        // once the first guard drops, the tool can be acquired again.
        assert!(RollGuard::acquire(&inflight, "codex").is_ok());
    }
}

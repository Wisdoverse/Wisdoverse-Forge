//! Agent container reconcile backstop.
//!
//! A periodic sweep that converges `agents.container_id` with Docker reality.
//! The roll/stop paths verify and reconcile their own post-conditions, but a
//! stop left `Unconfirmed` (daemon error during stop) or `StillRunning` does NOT
//! clear the row — by design, so a live container is never abandoned. This
//! worker closes that loop: for every container-runtime agent that still
//! references a container, it inspects the container; if it has since gone away,
//! it clears the stale reference and marks the agent offline.
//!
//! This mirrors the existing self-healing precedents — the participant-liveness
//! backstop and the clone-orphan sweep — but for agent container references,
//! which previously had no reconcile path at all.

use std::time::Duration;

use agentforge_core::{AgentId, OrgId, TenantScope, UserId, WorkspaceId};
use tokio::sync::watch;

use crate::health::AppState;
use crate::repositories::admin::AdminRepository;

/// Periodic reconcile sweep over container-runtime agents.
pub struct AgentContainerReconcileWorker {
    state: AppState,
    interval: Duration,
}

impl AgentContainerReconcileWorker {
    pub fn new(state: AppState, interval: Duration) -> Self {
        Self { state, interval }
    }

    /// Run until `shutdown` flips to `true`. The first sweep waits one interval
    /// so startup is not contended.
    pub async fn run(self, mut shutdown: watch::Receiver<bool>) {
        let mut ticker = tokio::time::interval(self.interval);
        // Skip the immediate first tick `interval` fires at t=0.
        ticker.tick().await;
        tracing::info!(interval_secs = self.interval.as_secs(), "agent container reconcile worker started");
        loop {
            tokio::select! {
                _ = ticker.tick() => self.sweep_once().await,
                changed = shutdown.changed() => {
                    if changed.is_err() || *shutdown.borrow() {
                        tracing::info!("agent container reconcile worker shutting down");
                        break;
                    }
                }
            }
        }
    }

    /// One reconcile pass. Best-effort: a per-agent failure is logged and the
    /// sweep continues; the next tick retries anything left.
    async fn sweep_once(&self) {
        // No Docker runtime on this deployment → nothing to reconcile.
        if self.state.docker.is_none() {
            return;
        }

        let admin = AdminRepository::new(self.state.pool.clone());
        let refs = match admin.container_agents_with_reference().await {
            Ok(refs) => refs,
            Err(err) => {
                tracing::warn!(error = ?err, "agent container reconcile: failed to list agents");
                return;
            }
        };

        let control = self.state.agent_container_control_service();
        let mut reconciled = 0usize;
        for agent_ref in refs {
            let scope = TenantScope::with_axes(
                OrgId::from(agent_ref.organization_id),
                UserId::from(agent_ref.user_id),
                agent_ref.workspace_id.map(WorkspaceId::from),
                None,
                None,
            );
            match control
                .reconcile_agent_if_container_absent(&scope, AgentId::from(agent_ref.id), &agent_ref.container_id)
                .await
            {
                Ok(true) => reconciled += 1,
                Ok(false) => {}
                Err(err) => {
                    tracing::warn!(error = ?err, agent_id = %agent_ref.id, "agent container reconcile: per-agent failure");
                }
            }
        }

        if reconciled > 0 {
            tracing::info!(reconciled, "agent container reconcile sweep cleared stale container references");
        }
    }
}

//! Read-side query service. Owns cross-aggregate filter queries that do
//! not belong on the write-side repository per CQRS hygiene.
//!
//! [`AgentQueryService`] wraps the tenant-scoped repository helpers that
//! filter by properties not exposed on the write path, keeping list/filter
//! logic separate from create/update/delete commands.

use crate::repositories::agent::{AgentListItem, AgentRepository};
use agentforge_core::{AppResult, RuntimeKind, TenantScope};
use sqlx::PgPool;

/// Read-side service for cross-cutting agent queries.
///
/// Construct via [`AgentQueryService::from_pool`] and pass a `TenantScope`
/// to every query so all results are constrained to the caller's organization.
pub struct AgentQueryService {
    repo: AgentRepository,
}

impl AgentQueryService {
    /// Build a service backed by the given connection pool.
    pub fn from_pool(pool: PgPool) -> Self {
        Self {
            repo: AgentRepository::new(pool),
        }
    }

    /// Return all agents whose `runtime_kind` matches `kind`, ordered by
    /// `created_at DESC`. Results are tenant-scoped to `scope.org_id()`.
    ///
    /// Uses `LIMIT` / `OFFSET` pagination; callers should pass reasonable
    /// upper bounds (e.g. `limit = 100`) to avoid unbounded result sets.
    pub async fn find_by_runtime_kind(
        &self,
        scope: &TenantScope,
        kind: RuntimeKind,
        limit: i64,
        offset: i64,
    ) -> AppResult<Vec<AgentListItem>> {
        self.repo
            .list_with_owner_filtered(scope, Some(kind), limit, offset)
            .await
    }
}

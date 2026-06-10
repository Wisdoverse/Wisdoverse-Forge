//! Pairing codes for one-command Host CLI join (agent aggregate submodule).
//!
//! Stores only the SHA-256 of each code. The claim lookup is intentionally
//! NOT tenant-scoped: claiming happens from an unauthenticated bootstrap
//! script where the code itself is the credential (same category as
//! login-by-email). The row pins the organization and agent, and the query
//! never matches anything except a live, unexpired hash.

use agentforge_core::AppResult;
use chrono::{DateTime, Utc};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

/// Everything the claim path needs, fetched in one query so no scopeless
/// follow-up reads against `agents` are required.
#[derive(Debug, sqlx::FromRow)]
pub(crate) struct JoinClaimRow {
    pub(crate) join_code_id: Uuid,
    pub(crate) agent_id: Uuid,
    pub(crate) organization_id: Uuid,
    pub(crate) runtime_kind: String,
    pub(crate) agent_name: Option<String>,
    pub(crate) cli_tool: Option<String>,
    pub(crate) model: Option<String>,
    pub(crate) hmac_secret: Option<String>,
    pub(crate) nats_connect_password: Option<String>,
}

#[derive(Clone)]
pub(crate) struct AgentJoinCodeRepository {
    pool: PgPool,
}

impl AgentJoinCodeRepository {
    pub(crate) fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Insert a freshly minted code hash inside the enrollment transaction.
    pub(crate) async fn store_in_tx(
        tx: &mut Transaction<'_, Postgres>,
        organization_id: Uuid,
        agent_id: Uuid,
        code_hash: &str,
        expires_at: DateTime<Utc>,
    ) -> AppResult<()> {
        sqlx::query(
            r#"INSERT INTO agent_join_codes (id, organization_id, agent_id, code_hash, expires_at)
               VALUES ($1, $2, $3, $4, $5)"#,
        )
        .bind(Uuid::now_v7())
        .bind(organization_id)
        .bind(agent_id)
        .bind(code_hash)
        .bind(expires_at)
        .execute(&mut **tx)
        .await?;
        Ok(())
    }

    /// Pool variant for the idempotent-replay path (agent already exists).
    pub(crate) async fn store(
        &self,
        organization_id: Uuid,
        agent_id: Uuid,
        code_hash: &str,
        expires_at: DateTime<Utc>,
    ) -> AppResult<()> {
        sqlx::query(
            r#"INSERT INTO agent_join_codes (id, organization_id, agent_id, code_hash, expires_at)
               VALUES ($1, $2, $3, $4, $5)"#,
        )
        .bind(Uuid::now_v7())
        .bind(organization_id)
        .bind(agent_id)
        .bind(code_hash)
        .bind(expires_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Resolve an unexpired code hash to its agent + stored credentials.
    /// Returns `None` for unknown or expired hashes — callers map both to the
    /// same opaque error.
    pub(crate) async fn find_valid_claim(&self, code_hash: &str) -> AppResult<Option<JoinClaimRow>> {
        let row = sqlx::query_as::<_, JoinClaimRow>(
            r#"SELECT j.id AS join_code_id,
                      a.id AS agent_id,
                      a.organization_id,
                      a.runtime_kind,
                      a.name AS agent_name,
                      a.cli_tool,
                      a.model,
                      a.hmac_secret,
                      a.nats_connect_password
                 FROM agent_join_codes j
                 JOIN agents a ON a.id = j.agent_id
                WHERE j.code_hash = $1
                  AND j.expires_at > NOW()"#,
        )
        .bind(code_hash)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    /// Audit trail: stamp first use and count every claim.
    pub(crate) async fn record_claim(&self, join_code_id: Uuid) -> AppResult<()> {
        sqlx::query(
            r#"UPDATE agent_join_codes
                  SET used_at = COALESCE(used_at, NOW()),
                      claim_count = claim_count + 1
                WHERE id = $1"#,
        )
        .bind(join_code_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}

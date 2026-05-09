//! Runtime capability repository — DB read-cache for the typed capability matrix.

use agentforge_core::{AppResult, RuntimeCapability};
use anyhow::anyhow;
use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

#[derive(Debug, Clone, FromRow)]
pub struct RuntimeCapabilityRow {
    pub id: Uuid,
    pub cli_tool: String,
    pub runtime_kind: String,
    pub max_context_tokens: i32,
    pub supports_skills_mount: bool,
    pub supports_hooks: bool,
    pub supports_subagents: bool,
    pub supports_mcp_bridge: bool,
    pub supports_terminal: bool,
    pub capability_profile: Value,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone)]
pub struct RuntimeCapabilityRepository {
    pool: PgPool,
}

impl RuntimeCapabilityRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn count(&self) -> AppResult<i64> {
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM runtime_capabilities")
            .fetch_one(&self.pool)
            .await
            .map_err(Into::into)
    }

    pub async fn list_all(&self) -> AppResult<Vec<RuntimeCapabilityRow>> {
        sqlx::query_as::<_, RuntimeCapabilityRow>(
            r#"SELECT id, cli_tool, runtime_kind, max_context_tokens,
                      supports_skills_mount, supports_hooks, supports_subagents,
                      supports_mcp_bridge, supports_terminal, capability_profile,
                      updated_at
                 FROM runtime_capabilities
                ORDER BY cli_tool ASC, runtime_kind ASC"#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(Into::into)
    }

    pub async fn insert_seed_profiles(&self, profiles: &[RuntimeCapability]) -> AppResult<u64> {
        let mut inserted = 0;
        let mut tx = self.pool.begin().await?;

        for profile in profiles {
            let Some(cli_tool) = profile.cli_tool else {
                continue;
            };
            let capability_profile =
                serde_json::to_value(profile).map_err(|err| anyhow!("serialize runtime capability seed: {err}"))?;
            let max_context_tokens = i32::try_from(profile.max_context_tokens)
                .map_err(|err| anyhow!("runtime capability max_context_tokens exceeds i32: {err}"))?;
            let result = sqlx::query(
                r#"INSERT INTO runtime_capabilities (
                       cli_tool, runtime_kind, max_context_tokens,
                       supports_skills_mount, supports_hooks, supports_subagents,
                       supports_mcp_bridge, supports_terminal, capability_profile
                   )
                   VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
                   ON CONFLICT (cli_tool, runtime_kind) DO NOTHING"#,
            )
            .bind(cli_tool.as_str())
            .bind(profile.runtime_kind.as_str())
            .bind(max_context_tokens)
            .bind(profile.supports_skills_mount)
            .bind(profile.supports_hooks)
            .bind(profile.supports_subagents)
            .bind(profile.supports_mcp_bridge)
            .bind(profile.supports_terminal)
            .bind(capability_profile)
            .execute(&mut *tx)
            .await?;
            inserted += result.rows_affected();
        }

        tx.commit().await?;
        Ok(inserted)
    }
}

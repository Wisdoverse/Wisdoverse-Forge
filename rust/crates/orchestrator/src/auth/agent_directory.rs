use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{Result, anyhow};
use async_trait::async_trait;
use sqlx::{PgPool, Row};
use tokio::sync::Mutex;

#[derive(Debug, Clone)]
pub struct AgentParticipant {
    pub participant_id: String,
    pub session_id: String,
    pub display_name: String,
    pub provider: String,
    pub org_id: String,
}

#[async_trait]
pub trait AgentDirectory: Send + Sync {
    async fn get_by_session(&self, org_id: &str, session_id: &str) -> Result<Option<AgentParticipant>>;
    async fn upsert_agent(
        &self,
        org_id: &str,
        session_id: &str,
        provider: &str,
        display_name: &str,
    ) -> Result<AgentParticipant>;
}

#[derive(Default)]
struct MemoryState {
    next_id: u64,
    by_session: HashMap<(String, String), AgentParticipant>,
}

#[derive(Default)]
pub struct MemoryAgentDirectory {
    state: Mutex<MemoryState>,
}

impl MemoryAgentDirectory {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl AgentDirectory for MemoryAgentDirectory {
    async fn get_by_session(&self, org_id: &str, session_id: &str) -> Result<Option<AgentParticipant>> {
        let state = self.state.lock().await;
        Ok(state.by_session.get(&(org_id.to_string(), session_id.to_string())).cloned())
    }

    async fn upsert_agent(
        &self,
        org_id: &str,
        session_id: &str,
        provider: &str,
        display_name: &str,
    ) -> Result<AgentParticipant> {
        let mut state = self.state.lock().await;
        let key = (org_id.to_string(), session_id.to_string());
        let display_name = normalize_display_name(display_name, session_id);
        let provider = normalize_provider(provider).unwrap_or_else(|| "unknown".to_string());

        if let Some(existing) = state.by_session.get_mut(&key) {
            existing.display_name = display_name.clone();
            existing.provider = provider.clone();
            return Ok(existing.clone());
        }

        state.next_id += 1;
        let participant = AgentParticipant {
            participant_id: format!("agent-{}", state.next_id),
            session_id: session_id.to_string(),
            display_name,
            provider,
            org_id: org_id.to_string(),
        };
        state.by_session.insert(key, participant.clone());
        Ok(participant)
    }
}

pub struct PgAgentDirectory {
    pool: PgPool,
}

impl PgAgentDirectory {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl AgentDirectory for PgAgentDirectory {
    async fn get_by_session(&self, org_id: &str, session_id: &str) -> Result<Option<AgentParticipant>> {
        let row = sqlx::query(
            "SELECT id::text AS id, agent_session_id, display_name, agent_provider, org_id               FROM participants WHERE org_id = $1 AND type = 'agent' AND agent_session_id = $2               ORDER BY created_at ASC LIMIT 1"
        )
        .bind(org_id)
        .bind(session_id)
        .fetch_optional(&self.pool)
        .await?;

        row.map(row_to_agent).transpose()
    }

    async fn upsert_agent(
        &self,
        org_id: &str,
        session_id: &str,
        provider: &str,
        display_name: &str,
    ) -> Result<AgentParticipant> {
        let display_name = normalize_display_name(display_name, session_id);
        let provider = normalize_provider(provider);

        if let Some(existing) = self.get_by_session(org_id, session_id).await? {
            sqlx::query(
                "UPDATE participants SET display_name = $1, agent_provider = $2, org_id = $3                  WHERE id = CAST($4 AS uuid) AND type = 'agent'"
            )
            .bind(&display_name)
            .bind(provider.clone())
            .bind(org_id)
            .bind(&existing.participant_id)
            .execute(&self.pool)
            .await?;

            return Ok(AgentParticipant {
                participant_id: existing.participant_id,
                session_id: session_id.to_string(),
                display_name,
                provider: provider.unwrap_or_else(|| "unknown".to_string()),
                org_id: org_id.to_string(),
            });
        }

        let row = sqlx::query(
            "INSERT INTO participants (type, display_name, agent_session_id, agent_provider, org_id)              VALUES ('agent', $1, $2, $3, $4)              RETURNING id::text AS id, agent_session_id, display_name, agent_provider, org_id"
        )
        .bind(&display_name)
        .bind(session_id)
        .bind(provider.clone())
        .bind(org_id)
        .fetch_one(&self.pool)
        .await?;

        row_to_agent(row)
    }
}

fn row_to_agent(row: sqlx::postgres::PgRow) -> Result<AgentParticipant> {
    let session_id = row
        .try_get::<Option<String>, _>("agent_session_id")?
        .ok_or_else(|| anyhow!("participant missing agent_session_id"))?;
    Ok(AgentParticipant {
        participant_id: row.try_get("id")?,
        session_id,
        display_name: row.try_get("display_name")?,
        provider: row.try_get::<Option<String>, _>("agent_provider")?.unwrap_or_else(|| "unknown".to_string()),
        org_id: row.try_get("org_id")?,
    })
}

fn normalize_display_name(display_name: &str, fallback: &str) -> String {
    let trimmed = display_name.trim();
    if trimmed.is_empty() { fallback.to_string() } else { trimmed.to_string() }
}

fn normalize_provider(provider: &str) -> Option<String> {
    match provider.trim() {
        "claude" | "gemini" | "codex" | "opencode" => Some(provider.trim().to_string()),
        _ => None,
    }
}

pub type SharedAgentDirectory = Arc<dyn AgentDirectory>;

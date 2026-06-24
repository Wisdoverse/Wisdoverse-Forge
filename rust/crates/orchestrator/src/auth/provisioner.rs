use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{Result, anyhow};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::postgres::PgRow;
use sqlx::{PgPool, Row};
use tokio::sync::Mutex;
use uuid::Uuid;

use super::AccessClaims;

#[derive(Debug, Clone)]
pub(crate) struct ParticipantRecord {
    id: String,
    external_user_id: String,
    display_name: String,
    org_id: String,
    created_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct ProvisionedParticipant {
    pub id: String,
    pub kind: &'static str,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ParticipantProfile {
    pub id: String,
    #[serde(rename = "type")]
    pub participant_type: String,
    pub user_id: String,
    pub display_name: String,
    pub org_id: String,
    pub created_at: DateTime<Utc>,
}

#[async_trait]
pub(crate) trait ParticipantStore: Send + Sync {
    async fn get_by_id(&self, org_id: &str, participant_id: &str) -> Result<Option<ParticipantRecord>>;
    async fn get_by_external_user_id(&self, org_id: &str, external_user_id: &str) -> Result<Option<ParticipantRecord>>;
    async fn list_humans(&self, org_id: &str) -> Result<Vec<ParticipantRecord>>;
    async fn create_human(
        &self,
        org_id: &str,
        external_user_id: &str,
        display_name: &str,
        preferred_id: Option<&str>,
    ) -> Result<ParticipantRecord>;
    async fn update_human(&self, participant: &ParticipantRecord) -> Result<()>;
}

pub struct Provisioner {
    store: Arc<dyn ParticipantStore>,
}

impl Provisioner {
    pub fn new() -> Self {
        Self::with_store(Arc::new(MemoryParticipantStore::new()))
    }

    pub(crate) fn with_store(store: Arc<dyn ParticipantStore>) -> Self {
        Self { store }
    }

    pub(crate) fn postgres(pool: PgPool) -> Self {
        Self::with_store(Arc::new(PgParticipantStore::new(pool)))
    }

    pub async fn ensure_participant(&self, claims: &AccessClaims) -> Result<ProvisionedParticipant> {
        let subject = claims.sub.trim();
        if subject.is_empty() {
            return Err(anyhow!("claims subject is required"));
        }
        let display_name = preferred_display_name(Some(&claims.display_name), Some(&claims.email), subject);
        Ok(self.ensure_human(&claims.org_id, subject, &display_name).await?.into_provisioned())
    }

    pub async fn ensure_internal_participant(&self, org_id: &str, user_id: &str) -> Result<ProvisionedParticipant> {
        Ok(self.ensure_internal_human(org_id, user_id, None).await?.into_provisioned())
    }

    pub async fn create_or_update_internal_participant(
        &self,
        org_id: &str,
        user_id: &str,
        display_name: Option<&str>,
    ) -> Result<ParticipantProfile> {
        Ok(self.ensure_internal_human(org_id, user_id, display_name).await?.into_profile())
    }

    pub async fn list_participants(&self, org_id: &str) -> Result<Vec<ParticipantProfile>> {
        let org_id = org_id.trim();
        if org_id.is_empty() {
            return Err(anyhow!("organization context is required"));
        }

        let participants = self.store.list_humans(org_id).await?;
        Ok(participants.into_iter().map(ParticipantRecord::into_profile).collect())
    }

    async fn ensure_internal_human(
        &self,
        org_id: &str,
        user_id: &str,
        display_name: Option<&str>,
    ) -> Result<ParticipantRecord> {
        let external_user_id = if user_id.trim().is_empty() { "internal" } else { user_id.trim() };
        let display_name = display_name.map(str::trim).filter(|value| !value.is_empty()).unwrap_or(external_user_id);
        self.ensure_human(org_id, external_user_id, display_name).await
    }

    async fn ensure_human(
        &self,
        org_id: &str,
        external_user_id: &str,
        display_name: &str,
    ) -> Result<ParticipantRecord> {
        if org_id.trim().is_empty() {
            return Err(anyhow!("organization context is required"));
        }
        let external_user_id = external_user_id.trim();
        if external_user_id.is_empty() {
            return Err(anyhow!("external user id is required"));
        }
        let display_name = if display_name.trim().is_empty() { external_user_id } else { display_name.trim() };

        if Uuid::parse_str(external_user_id).is_ok()
            && let Some(existing) = self.store.get_by_id(org_id, external_user_id).await?
        {
            return self.refresh_profile(existing, org_id, external_user_id, display_name).await;
        }

        if let Some(existing) = self.store.get_by_external_user_id(org_id, external_user_id).await? {
            return self.refresh_profile(existing, org_id, external_user_id, display_name).await;
        }

        let preferred_id = if Uuid::parse_str(external_user_id).is_ok() { Some(external_user_id) } else { None };
        self.store.create_human(org_id, external_user_id, display_name, preferred_id).await
    }

    async fn refresh_profile(
        &self,
        existing: ParticipantRecord,
        org_id: &str,
        external_user_id: &str,
        display_name: &str,
    ) -> Result<ParticipantRecord> {
        if existing.org_id == org_id
            && existing.external_user_id == external_user_id
            && existing.display_name == display_name
        {
            return Ok(existing);
        }

        let updated = ParticipantRecord {
            id: existing.id.clone(),
            external_user_id: external_user_id.to_string(),
            display_name: display_name.to_string(),
            org_id: org_id.to_string(),
            created_at: existing.created_at,
        };
        self.store.update_human(&updated).await?;
        Ok(updated)
    }
}

impl Default for Provisioner {
    fn default() -> Self {
        Self::new()
    }
}

impl ParticipantRecord {
    fn into_provisioned(self) -> ProvisionedParticipant {
        ProvisionedParticipant { id: self.id, kind: "human", created_at: self.created_at }
    }

    fn into_profile(self) -> ParticipantProfile {
        ParticipantProfile {
            id: self.id,
            participant_type: "human".to_string(),
            user_id: self.external_user_id,
            display_name: self.display_name,
            org_id: self.org_id,
            created_at: self.created_at,
        }
    }
}

fn preferred_display_name(display_name: Option<&str>, email: Option<&str>, fallback: &str) -> String {
    display_name
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .or_else(|| email.map(str::trim).filter(|value| !value.is_empty()))
        .unwrap_or(fallback)
        .to_string()
}

#[derive(Default)]
struct MemoryParticipantState {
    next_id: u64,
    by_id: HashMap<String, ParticipantRecord>,
    by_external: HashMap<(String, String), String>,
}

#[derive(Default)]
pub struct MemoryParticipantStore {
    state: Mutex<MemoryParticipantState>,
}

impl MemoryParticipantStore {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl ParticipantStore for MemoryParticipantStore {
    async fn get_by_id(&self, org_id: &str, participant_id: &str) -> Result<Option<ParticipantRecord>> {
        let state = self.state.lock().await;
        Ok(state.by_id.get(participant_id).filter(|record| record.org_id == org_id).cloned())
    }

    async fn get_by_external_user_id(&self, org_id: &str, external_user_id: &str) -> Result<Option<ParticipantRecord>> {
        let state = self.state.lock().await;
        let Some(participant_id) = state.by_external.get(&(org_id.to_string(), external_user_id.to_string())) else {
            return Ok(None);
        };
        Ok(state.by_id.get(participant_id).cloned())
    }

    async fn list_humans(&self, org_id: &str) -> Result<Vec<ParticipantRecord>> {
        let state = self.state.lock().await;
        let mut participants: Vec<_> = state.by_id.values().filter(|record| record.org_id == org_id).cloned().collect();
        participants.sort_by(|left, right| left.created_at.cmp(&right.created_at).then_with(|| left.id.cmp(&right.id)));
        Ok(participants)
    }

    async fn create_human(
        &self,
        org_id: &str,
        external_user_id: &str,
        display_name: &str,
        preferred_id: Option<&str>,
    ) -> Result<ParticipantRecord> {
        let mut state = self.state.lock().await;
        let participant_id = match preferred_id {
            Some(participant_id) => participant_id.to_string(),
            None => {
                state.next_id += 1;
                format!("p-{}", state.next_id)
            }
        };
        let record = ParticipantRecord {
            id: participant_id.clone(),
            external_user_id: external_user_id.to_string(),
            display_name: display_name.to_string(),
            org_id: org_id.to_string(),
            created_at: Utc::now(),
        };
        state.by_external.insert((org_id.to_string(), external_user_id.to_string()), participant_id.clone());
        state.by_id.insert(participant_id, record.clone());
        Ok(record)
    }

    async fn update_human(&self, participant: &ParticipantRecord) -> Result<()> {
        let mut state = self.state.lock().await;
        let Some(existing) = state.by_id.get(&participant.id).cloned() else {
            return Err(anyhow!("participant not found"));
        };
        state.by_external.remove(&(existing.org_id, existing.external_user_id));
        state
            .by_external
            .insert((participant.org_id.clone(), participant.external_user_id.clone()), participant.id.clone());
        state.by_id.insert(participant.id.clone(), participant.clone());
        Ok(())
    }
}

pub struct PgParticipantStore {
    pool: PgPool,
}

impl PgParticipantStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ParticipantStore for PgParticipantStore {
    async fn get_by_id(&self, org_id: &str, participant_id: &str) -> Result<Option<ParticipantRecord>> {
        let row = sqlx::query(
            "SELECT id::text AS id, casdoor_user_id, display_name, org_id, created_at              FROM participants WHERE id = CAST($1 AS uuid) AND org_id = $2 AND type = 'human'"
        )
        .bind(participant_id)
        .bind(org_id)
        .fetch_optional(&self.pool)
        .await?;
        row.map(row_to_participant).transpose()
    }

    async fn get_by_external_user_id(&self, org_id: &str, external_user_id: &str) -> Result<Option<ParticipantRecord>> {
        let row = sqlx::query(
            "SELECT id::text AS id, casdoor_user_id, display_name, org_id, created_at              FROM participants WHERE org_id = $1 AND casdoor_user_id = $2 AND type = 'human' ORDER BY created_at ASC LIMIT 1"
        )
        .bind(org_id)
        .bind(external_user_id)
        .fetch_optional(&self.pool)
        .await?;
        row.map(row_to_participant).transpose()
    }

    async fn list_humans(&self, org_id: &str) -> Result<Vec<ParticipantRecord>> {
        let rows = sqlx::query(
            "SELECT id::text AS id, casdoor_user_id, display_name, org_id, created_at              FROM participants WHERE org_id = $1 AND type = 'human' ORDER BY created_at ASC, id ASC"
        )
        .bind(org_id)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(row_to_participant).collect()
    }

    async fn create_human(
        &self,
        org_id: &str,
        external_user_id: &str,
        display_name: &str,
        preferred_id: Option<&str>,
    ) -> Result<ParticipantRecord> {
        let row = if let Some(participant_id) = preferred_id {
            sqlx::query(
                "INSERT INTO participants (id, type, display_name, casdoor_user_id, org_id)                  VALUES (CAST($1 AS uuid), 'human', $2, $3, $4)                  RETURNING id::text AS id, casdoor_user_id, display_name, org_id, created_at"
            )
            .bind(participant_id)
            .bind(display_name)
            .bind(external_user_id)
            .bind(org_id)
            .fetch_one(&self.pool)
            .await?
        } else {
            sqlx::query(
                "INSERT INTO participants (type, display_name, casdoor_user_id, org_id)                  VALUES ('human', $1, $2, $3)                  RETURNING id::text AS id, casdoor_user_id, display_name, org_id, created_at"
            )
            .bind(display_name)
            .bind(external_user_id)
            .bind(org_id)
            .fetch_one(&self.pool)
            .await?
        };
        row_to_participant(row)
    }

    async fn update_human(&self, participant: &ParticipantRecord) -> Result<()> {
        let result = sqlx::query(
            "UPDATE participants SET display_name = $1, casdoor_user_id = $2, org_id = $3              WHERE id = CAST($4 AS uuid) AND type = 'human'"
        )
        .bind(&participant.display_name)
        .bind(&participant.external_user_id)
        .bind(&participant.org_id)
        .bind(&participant.id)
        .execute(&self.pool)
        .await?;
        if result.rows_affected() == 0 {
            return Err(anyhow!("participant not found"));
        }
        Ok(())
    }
}

fn row_to_participant(row: PgRow) -> Result<ParticipantRecord> {
    let external_user_id = row
        .try_get::<Option<String>, _>("casdoor_user_id")?
        .ok_or_else(|| anyhow!("participant missing casdoor_user_id"))?;
    Ok(ParticipantRecord {
        id: row.try_get("id")?,
        external_user_id,
        display_name: row.try_get("display_name")?,
        org_id: row.try_get("org_id")?,
        created_at: row.try_get("created_at")?,
    })
}

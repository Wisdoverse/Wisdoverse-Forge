use std::collections::HashMap;
use std::str::FromStr;
use std::sync::atomic::{AtomicU64, Ordering};

use async_trait::async_trait;
use chrono::Utc;
use sqlx::postgres::PgRow;
use sqlx::{PgPool, Row};
use tokio::sync::Mutex;

use super::errors::{Result, TeamError};
use super::model::{Team, TeamMember, TeamRole, TeamWithMembers, UpdateTeamRequest};
use super::store::Store;

pub struct MemoryStore {
    seq: AtomicU64,
    teams: Mutex<HashMap<String, TeamWithMembers>>,
}

impl MemoryStore {
    pub fn new() -> Self {
        Self { seq: AtomicU64::new(1), teams: Mutex::new(HashMap::new()) }
    }
}

impl Default for MemoryStore {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Store for MemoryStore {
    async fn create(&self, team: &mut Team) -> Result<()> {
        let now = Utc::now();
        let id = format!("team-{}", self.seq.fetch_add(1, Ordering::Relaxed));
        team.id = id.clone();
        team.created_at = now;
        team.updated_at = now;
        self.teams.lock().await.insert(id, TeamWithMembers { team: team.clone(), members: Vec::new() });
        Ok(())
    }

    async fn get_by_id(&self, id: &str, org_id: &str) -> Result<TeamWithMembers> {
        self.teams.lock().await.get(id).filter(|team| team.team.org_id == org_id).cloned().ok_or(TeamError::NotFound)
    }

    async fn list(&self, org_id: &str) -> Result<Vec<Team>> {
        let mut teams: Vec<Team> = self
            .teams
            .lock()
            .await
            .values()
            .filter(|team| team.team.org_id == org_id)
            .map(|team| team.team.clone())
            .collect();
        teams.sort_by(|left, right| left.name.cmp(&right.name));
        Ok(teams)
    }

    async fn update(&self, id: &str, org_id: &str, req: UpdateTeamRequest) -> Result<()> {
        let mut teams = self.teams.lock().await;
        let Some(team) = teams.get_mut(id).filter(|team| team.team.org_id == org_id) else {
            return Err(TeamError::NotFound);
        };
        if let Some(name) = req.name {
            team.team.name = name;
        }
        team.team.updated_at = Utc::now();
        Ok(())
    }

    async fn delete(&self, id: &str, org_id: &str) -> Result<()> {
        let mut teams = self.teams.lock().await;
        let Some(existing) = teams.get(id) else {
            return Err(TeamError::NotFound);
        };
        if existing.team.org_id != org_id {
            return Err(TeamError::NotFound);
        }
        teams.remove(id);
        Ok(())
    }

    async fn add_member(&self, team_id: &str, member: &mut TeamMember) -> Result<()> {
        let mut teams = self.teams.lock().await;
        let Some(team) = teams.get_mut(team_id) else {
            return Err(TeamError::NotFound);
        };
        if team.members.iter().any(|existing| existing.participant_id == member.participant_id) {
            return Err(TeamError::InvalidInput("member already exists".to_string()));
        }
        member.team_id = team_id.to_string();
        member.joined_at = Utc::now();
        team.members.push(member.clone());
        team.team.updated_at = Utc::now();
        Ok(())
    }

    async fn remove_member(&self, team_id: &str, participant_id: &str) -> Result<()> {
        let mut teams = self.teams.lock().await;
        let Some(team) = teams.get_mut(team_id) else {
            return Err(TeamError::NotFound);
        };
        let before = team.members.len();
        team.members.retain(|member| member.participant_id != participant_id);
        if team.members.len() == before {
            return Err(TeamError::NotFound);
        }
        team.team.updated_at = Utc::now();
        Ok(())
    }
}

pub struct PgTeamStore {
    pool: PgPool,
}

impl PgTeamStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl Store for PgTeamStore {
    async fn create(&self, team: &mut Team) -> Result<()> {
        let row = sqlx::query(
            "INSERT INTO teams (name, org_id, created_by) VALUES ($1, $2, $3)                      RETURNING id::text AS id, created_at, updated_at"
        )
        .bind(&team.name)
        .bind(&team.org_id)
        .bind(&team.created_by)
        .fetch_one(&self.pool)
        .await
        .map_err(|err| TeamError::Internal(format!("insert team: {err}")))?;

        team.id = row.try_get("id").map_err(|err| TeamError::Internal(format!("read team id: {err}")))?;
        team.created_at =
            row.try_get("created_at").map_err(|err| TeamError::Internal(format!("read team created_at: {err}")))?;
        team.updated_at =
            row.try_get("updated_at").map_err(|err| TeamError::Internal(format!("read team updated_at: {err}")))?;
        Ok(())
    }

    async fn get_by_id(&self, id: &str, org_id: &str) -> Result<TeamWithMembers> {
        let row = sqlx::query(
            "SELECT id::text AS id, name, org_id, created_by, created_at, updated_at                      FROM teams WHERE id = CAST($1 AS uuid) AND org_id = $2"
        )
        .bind(id)
        .bind(org_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|err| TeamError::Internal(format!("get team: {err}")))?
        .ok_or(TeamError::NotFound)?;

        let team = row_to_team(&row)?;
        let rows = sqlx::query(
            "SELECT team_id::text AS team_id, participant_id::text AS participant_id, role, joined_at                      FROM team_members WHERE team_id = CAST($1 AS uuid) ORDER BY joined_at ASC"
        )
        .bind(id)
        .fetch_all(&self.pool)
        .await
        .map_err(|err| TeamError::Internal(format!("get team members: {err}")))?;
        let members = rows.iter().map(row_to_member).collect::<Result<Vec<_>>>()?;
        Ok(TeamWithMembers { team, members })
    }

    async fn list(&self, org_id: &str) -> Result<Vec<Team>> {
        let rows = sqlx::query(
            "SELECT id::text AS id, name, org_id, created_by, created_at, updated_at                      FROM teams WHERE org_id = $1 ORDER BY name"
        )
        .bind(org_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|err| TeamError::Internal(format!("list teams: {err}")))?;
        rows.iter().map(row_to_team).collect()
    }

    async fn update(&self, id: &str, org_id: &str, req: UpdateTeamRequest) -> Result<()> {
        let Some(name) = req.name else {
            return Ok(());
        };
        let result =
            sqlx::query("UPDATE teams SET name = $1, updated_at = NOW() WHERE id = CAST($2 AS uuid) AND org_id = $3")
                .bind(name)
                .bind(id)
                .bind(org_id)
                .execute(&self.pool)
                .await
                .map_err(|err| TeamError::Internal(format!("update team: {err}")))?;
        if result.rows_affected() == 0 {
            return Err(TeamError::NotFound);
        }
        Ok(())
    }

    async fn delete(&self, id: &str, org_id: &str) -> Result<()> {
        let result = sqlx::query("DELETE FROM teams WHERE id = CAST($1 AS uuid) AND org_id = $2")
            .bind(id)
            .bind(org_id)
            .execute(&self.pool)
            .await
            .map_err(|err| TeamError::Internal(format!("delete team: {err}")))?;
        if result.rows_affected() == 0 {
            return Err(TeamError::NotFound);
        }
        Ok(())
    }

    async fn add_member(&self, team_id: &str, member: &mut TeamMember) -> Result<()> {
        let row = sqlx::query(
            "INSERT INTO team_members (team_id, participant_id, role)                      VALUES (CAST($1 AS uuid), CAST($2 AS uuid), $3)                      RETURNING team_id::text AS team_id, participant_id::text AS participant_id, role, joined_at"
        )
        .bind(team_id)
        .bind(&member.participant_id)
        .bind(member.role.as_str())
        .fetch_one(&self.pool)
        .await
        .map_err(|err| TeamError::Internal(format!("add team member: {err}")))?;
        *member = row_to_member(&row)?;
        Ok(())
    }

    async fn remove_member(&self, team_id: &str, participant_id: &str) -> Result<()> {
        let result = sqlx::query(
            "DELETE FROM team_members WHERE team_id = CAST($1 AS uuid) AND participant_id = CAST($2 AS uuid)",
        )
        .bind(team_id)
        .bind(participant_id)
        .execute(&self.pool)
        .await
        .map_err(|err| TeamError::Internal(format!("remove team member: {err}")))?;
        if result.rows_affected() == 0 {
            return Err(TeamError::NotFound);
        }
        Ok(())
    }
}

fn row_to_team(row: &PgRow) -> Result<Team> {
    Ok(Team {
        id: row.try_get("id").map_err(|err| TeamError::Internal(format!("read team id: {err}")))?,
        name: row.try_get("name").map_err(|err| TeamError::Internal(format!("read team name: {err}")))?,
        org_id: row.try_get("org_id").map_err(|err| TeamError::Internal(format!("read org_id: {err}")))?,
        created_by: row.try_get("created_by").map_err(|err| TeamError::Internal(format!("read created_by: {err}")))?,
        created_at: row.try_get("created_at").map_err(|err| TeamError::Internal(format!("read created_at: {err}")))?,
        updated_at: row.try_get("updated_at").map_err(|err| TeamError::Internal(format!("read updated_at: {err}")))?,
    })
}

fn row_to_member(row: &PgRow) -> Result<TeamMember> {
    let role = row.try_get::<String, _>("role").map_err(|err| TeamError::Internal(format!("read role: {err}")))?;
    Ok(TeamMember {
        team_id: row.try_get("team_id").map_err(|err| TeamError::Internal(format!("read team_id: {err}")))?,
        participant_id: row
            .try_get("participant_id")
            .map_err(|err| TeamError::Internal(format!("read participant_id: {err}")))?,
        role: TeamRole::from_str(&role).map_err(TeamError::Internal)?,
        joined_at: row.try_get("joined_at").map_err(|err| TeamError::Internal(format!("read joined_at: {err}")))?,
    })
}

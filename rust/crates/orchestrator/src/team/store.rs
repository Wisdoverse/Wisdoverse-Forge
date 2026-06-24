use async_trait::async_trait;

use super::errors::Result;
use super::model::{Team, TeamMember, TeamWithMembers, UpdateTeamRequest};

#[async_trait]
pub trait Store: Send + Sync {
    async fn create(&self, team: &mut Team) -> Result<()>;
    async fn get_by_id(&self, id: &str, org_id: &str) -> Result<TeamWithMembers>;
    async fn list(&self, org_id: &str) -> Result<Vec<Team>>;
    async fn update(&self, id: &str, org_id: &str, req: UpdateTeamRequest) -> Result<()>;
    async fn delete(&self, id: &str, org_id: &str) -> Result<()>;
    async fn add_member(&self, team_id: &str, member: &mut TeamMember) -> Result<()>;
    async fn remove_member(&self, team_id: &str, participant_id: &str) -> Result<()>;
}

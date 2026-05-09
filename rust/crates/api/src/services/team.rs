//! Team service — business logic and validation.

use agentforge_core::{AppResult, ErrorKind, TeamId, TenantScope};
use agentforge_db::entities::Team;

use crate::repositories::team::TeamRepository;

/// Business logic layer for team operations.
pub struct TeamService {
    repo: TeamRepository,
}

impl TeamService {
    pub fn new(repo: TeamRepository) -> Self {
        Self { repo }
    }

    /// List teams with pagination. Limit is capped at 100.
    pub async fn list(&self, scope: &TenantScope, limit: i64, offset: i64) -> AppResult<Vec<Team>> {
        let limit = limit.clamp(1, 100);
        let offset = offset.max(0);
        self.repo.list(scope, limit, offset).await
    }

    /// Get a single team by ID.
    pub async fn get(&self, scope: &TenantScope, id: TeamId) -> AppResult<Team> {
        self.repo.find_by_id(scope, id).await
    }

    /// Create a new team with validated name.
    pub async fn create(&self, scope: &TenantScope, name: &str) -> AppResult<Team> {
        Self::validate_name(name)?;
        self.repo.create(scope, name).await
    }

    /// Update a team's name.
    pub async fn update(&self, scope: &TenantScope, id: TeamId, name: &str) -> AppResult<Team> {
        Self::validate_name(name)?;
        self.repo.update(scope, id, name).await
    }

    /// Soft-delete a team.
    pub async fn delete(&self, scope: &TenantScope, id: TeamId) -> AppResult<()> {
        self.repo.delete(scope, id).await
    }

    /// Validate team name: 1-255 characters.
    fn validate_name(name: &str) -> AppResult<()> {
        if name.is_empty() || name.len() > 255 {
            return Err(ErrorKind::Validation("name must be between 1 and 255 characters".into()).into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_names() {
        assert!(TeamService::validate_name("A").is_ok());
        assert!(TeamService::validate_name("Engineering").is_ok());
        assert!(TeamService::validate_name(&"a".repeat(255)).is_ok());
    }

    #[test]
    fn invalid_names() {
        assert!(TeamService::validate_name("").is_err());
        assert!(TeamService::validate_name(&"a".repeat(256)).is_err());
    }

    #[test]
    fn limit_clamping() {
        assert_eq!(0_i64.clamp(1, 100), 1);
        assert_eq!(200_i64.clamp(1, 100), 100);
    }

    #[test]
    fn offset_floor() {
        let negative_offset = -10_i64;
        let positive_offset = 50_i64;
        assert_eq!(negative_offset.max(0), 0);
        assert_eq!(positive_offset.max(0), 50);
    }
}

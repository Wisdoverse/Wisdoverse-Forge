use async_trait::async_trait;

use super::errors::Result;
use super::model::{AuditFilter, AuditLog};

#[async_trait]
pub trait Store: Send + Sync {
    async fn create(&self, log: &mut AuditLog) -> Result<()>;
    async fn list(&self, filter: AuditFilter) -> Result<(Vec<AuditLog>, usize)>;
    async fn export(&self, filter: AuditFilter) -> Result<Vec<AuditLog>>;
}

//! Per-agent NATS connect password lookup (issue #38 phase 2).
//!
//! The NATS auth callout service receives a `user=<agent_uuid>`, `pass=<pw>`
//! pair from the sidecar at CONNECT time and must validate it against
//! `agents.nats_connect_password`. This trait models that lookup.
//!
//! Modeled as a trait so tests can inject fixed keys without a DB. **Not
//! tenant-scoped** — the callout runs in a worker context without
//! `TenantScope`, and the `agent_id` comes from the CONNECT credentials the
//! sidecar presents, which is already the identity we verify against. The
//! auth check IS the tenant check: if the password matches, the caller
//! has proven they are this agent, and we mint a JWT whose subject
//! permissions are scoped to `<agent_uuid>` — no other agent's subjects are
//! reachable even if the caller later lies about their org.
//!
//! Returning `Ok(None)` for an unknown agent is the expected path for
//! forged users and for agents spawned before migration 028; the callout
//! treats it as a uniform deny (same response shape as password_mismatch to
//! avoid agent-UUID enumeration).

use anyhow::Result;
use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

/// Fetches the per-agent NATS connect password persisted at spawn time.
#[async_trait]
pub trait NatsConnectPasswordLookup: Clone + Send + Sync + 'static {
    async fn find_password(&self, agent_id: Uuid) -> Result<Option<String>>;
}

/// Production `NatsConnectPasswordLookup` backed by
/// `agents.nats_connect_password`. The column is nullable — we treat
/// "missing row" and "row with NULL password" the same, returning `Ok(None)`
/// so the caller can emit a uniform deny.
#[derive(Clone)]
pub struct SqlxNatsConnectPasswordLookup {
    pool: PgPool,
}

impl SqlxNatsConnectPasswordLookup {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl NatsConnectPasswordLookup for SqlxNatsConnectPasswordLookup {
    async fn find_password(&self, agent_id: Uuid) -> Result<Option<String>> {
        let row: Option<(Option<String>,)> =
            sqlx::query_as(r#"SELECT nats_connect_password FROM agents WHERE id = $1 LIMIT 1"#)
                .bind(agent_id)
                .fetch_optional(&self.pool)
                .await?;
        Ok(row.and_then(|r| r.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Arc;
    use tokio::sync::Mutex;

    #[derive(Clone, Default)]
    struct FakeLookup {
        map: Arc<Mutex<HashMap<Uuid, Option<String>>>>,
    }

    impl FakeLookup {
        async fn insert(&self, id: Uuid, password: Option<String>) {
            self.map.lock().await.insert(id, password);
        }
    }

    #[async_trait]
    impl NatsConnectPasswordLookup for FakeLookup {
        async fn find_password(&self, agent_id: Uuid) -> Result<Option<String>> {
            Ok(self.map.lock().await.get(&agent_id).cloned().flatten())
        }
    }

    #[tokio::test]
    async fn returns_none_for_unknown_agent() {
        let lookup = FakeLookup::default();
        assert_eq!(lookup.find_password(Uuid::new_v4()).await.unwrap(), None);
    }

    #[tokio::test]
    async fn returns_password_for_known_agent() {
        let lookup = FakeLookup::default();
        let agent_id = Uuid::new_v4();
        lookup.insert(agent_id, Some("secret-uuid-v4".to_string())).await;
        assert_eq!(lookup.find_password(agent_id).await.unwrap(), Some("secret-uuid-v4".to_string()));
    }

    #[tokio::test]
    async fn returns_none_for_agent_with_null_password() {
        let lookup = FakeLookup::default();
        let agent_id = Uuid::new_v4();
        lookup.insert(agent_id, None).await;
        assert_eq!(lookup.find_password(agent_id).await.unwrap(), None);
    }
}

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

/// The per-agent identity the auth callout needs to mint a scoped User JWT:
/// the connect password to verify the CONNECT against, plus the agent's
/// `runtime_kind` so the granted subject allowlist can be namespaced by kind
/// (issue #457). Both are read from a single `agents` row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentNatsIdentity {
    /// `agents.nats_connect_password` — verified with a constant-time compare.
    pub password: String,
    /// `agents.runtime_kind` — `container` | `cli` | `api`. NOT NULL post
    /// migration 062; kept as a raw string here (the callout normalises it via
    /// `RuntimeKind::parse_legacy`) so an unrecognised value degrades to a safe
    /// default rather than breaking auth.
    pub runtime_kind: String,
}

/// Fetches the per-agent NATS connect identity persisted at spawn time.
#[async_trait]
pub trait NatsConnectPasswordLookup: Clone + Send + Sync + 'static {
    /// Returns the agent's NATS identity, or `None` when the row is missing or
    /// has a NULL password. The two are deliberately indistinguishable to the
    /// caller so denies don't leak which agent UUIDs exist.
    async fn find_identity(&self, agent_id: Uuid) -> Result<Option<AgentNatsIdentity>>;
}

/// Production `NatsConnectPasswordLookup` backed by `agents`. The password
/// column is nullable — we treat "missing row" and "row with NULL password"
/// the same, returning `Ok(None)` so the caller can emit a uniform deny.
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
    async fn find_identity(&self, agent_id: Uuid) -> Result<Option<AgentNatsIdentity>> {
        let row: Option<(Option<String>, String)> =
            sqlx::query_as(r#"SELECT nats_connect_password, runtime_kind FROM agents WHERE id = $1 LIMIT 1"#)
                .bind(agent_id)
                .fetch_optional(&self.pool)
                .await?;
        Ok(row
            .and_then(|(password, runtime_kind)| password.map(|password| AgentNatsIdentity { password, runtime_kind })))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Arc;
    use tokio::sync::Mutex;

    /// Map value `None` models a missing row / NULL password; `Some(identity)`
    /// models a present row.
    #[derive(Clone, Default)]
    struct FakeLookup {
        map: Arc<Mutex<HashMap<Uuid, Option<AgentNatsIdentity>>>>,
    }

    impl FakeLookup {
        async fn insert(&self, id: Uuid, identity: Option<AgentNatsIdentity>) {
            self.map.lock().await.insert(id, identity);
        }
    }

    #[async_trait]
    impl NatsConnectPasswordLookup for FakeLookup {
        async fn find_identity(&self, agent_id: Uuid) -> Result<Option<AgentNatsIdentity>> {
            Ok(self.map.lock().await.get(&agent_id).cloned().flatten())
        }
    }

    fn identity(password: &str, runtime_kind: &str) -> AgentNatsIdentity {
        AgentNatsIdentity { password: password.to_string(), runtime_kind: runtime_kind.to_string() }
    }

    #[tokio::test]
    async fn returns_none_for_unknown_agent() {
        let lookup = FakeLookup::default();
        assert_eq!(lookup.find_identity(Uuid::new_v4()).await.unwrap(), None);
    }

    #[tokio::test]
    async fn returns_identity_for_known_agent() {
        let lookup = FakeLookup::default();
        let agent_id = Uuid::new_v4();
        lookup.insert(agent_id, Some(identity("secret-uuid-v4", "cli"))).await;
        assert_eq!(lookup.find_identity(agent_id).await.unwrap(), Some(identity("secret-uuid-v4", "cli")));
    }

    #[tokio::test]
    async fn returns_none_for_agent_with_null_password() {
        let lookup = FakeLookup::default();
        let agent_id = Uuid::new_v4();
        lookup.insert(agent_id, None).await;
        assert_eq!(lookup.find_identity(agent_id).await.unwrap(), None);
    }
}

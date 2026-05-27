//! Storage for the `enrollment_idempotency` table used by Host CLI enrollment.

use agentforge_core::{AppError, AppResult};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

pub struct EnrollmentIdempotencyRepository {
    pool: PgPool,
}

impl EnrollmentIdempotencyRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Returns the previously-enrolled `agent_id` if the (org_id, user_id, key)
    /// triple is present and not expired.
    pub async fn lookup(
        &self,
        org_id: Uuid,
        user_id: Uuid,
        key: &str,
    ) -> AppResult<Option<Uuid>> {
        let row: Option<(Uuid,)> = sqlx::query_as(
            "SELECT agent_id FROM enrollment_idempotency
             WHERE org_id = $1 AND user_id = $2 AND key = $3 AND expires_at > NOW()",
        )
        .bind(org_id)
        .bind(user_id)
        .bind(key)
        .fetch_optional(&self.pool)
        .await
        .map_err(AppError::from)?;
        Ok(row.map(|(id,)| id))
    }

    /// Inserts the idempotency record within the caller's transaction.
    ///
    /// `ON CONFLICT DO NOTHING` because a concurrent first-writer wins;
    /// callers should `lookup` first to detect replay.
    pub async fn store_in_tx(
        tx: &mut Transaction<'_, Postgres>,
        org_id: Uuid,
        user_id: Uuid,
        key: &str,
        agent_id: Uuid,
    ) -> AppResult<()> {
        sqlx::query(
            "INSERT INTO enrollment_idempotency (org_id, user_id, key, agent_id, expires_at)
             VALUES ($1, $2, $3, $4, NOW() + INTERVAL '24 hours')
             ON CONFLICT (org_id, user_id, key) DO NOTHING",
        )
        .bind(org_id)
        .bind(user_id)
        .bind(key)
        .bind(agent_id)
        .execute(&mut **tx)
        .await
        .map(|_| ())
        .map_err(AppError::from)
    }
}

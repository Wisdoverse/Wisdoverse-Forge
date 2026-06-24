//! Container CLI credential repository — reads the legacy `user_cli_credentials` table
//! populated by the TS `CliCredentialService` (`server/src/modules/cli-credential`).
//!
//! Scoping note: the legacy schema is per-user and has no `organization_id`
//! column (credentials follow the user across orgs), so we filter by
//! `scope.user_id()` rather than `organization_id`. `TenantScope` is still
//! passed in to enforce the pattern — a future migration can add org scoping
//! without changing this signature.
//!
//! Decryption is deliberately kept out of this layer so the repo remains a
//! thin SQL wrapper; see `crate::services::cli_credential::CliCredentialService`.
//! Write paths (upsert, delete) live here so both the OAuth proxy and the
//! manual upload endpoint can reuse them.

use agentforge_core::{AppResult, TenantScope};
use sqlx::PgPool;

/// Row shape returned by `find_encrypted_with_revocation`: the ciphertext,
/// plus the three revocation-tracking columns. Aliased so the public API
/// doesn't return a bare 4-tuple (and so clippy's type-complexity lint
/// stays happy).
pub type EncryptedWithRevocation = (String, Option<chrono::DateTime<chrono::Utc>>, Option<String>, i32);

#[derive(Clone)]
pub struct CliCredentialRepository {
    pool: PgPool,
}

impl CliCredentialRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Return the base64-encoded AES-GCM ciphertext for the given user+tool,
    /// or `None` when nothing is stored. `cli_tool` is the canonical slug
    /// (`claude` / `codex` / `gemini` / `opencode`).
    pub async fn find_encrypted(&self, scope: &TenantScope, cli_tool: &str) -> AppResult<Option<String>> {
        let row: Option<(String,)> = sqlx::query_as(
            r#"SELECT encrypted_credentials FROM user_cli_credentials WHERE user_id = $1 AND cli_tool = $2"#,
        )
        .bind(scope.user_id().as_uuid())
        .bind(cli_tool)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|(c,)| c))
    }

    /// Upsert a pre-encrypted ciphertext blob. Encryption happens in the
    /// service layer so this repo never sees the decryption key, matching the
    /// legacy TS `CliCredentialRepository.upsert` signature.
    ///
    /// Clears revocation markers on the `DO UPDATE SET` path so a successful
    /// re-auth (complete_manual / server-callback / manual file map upload /
    /// credential-sync publish) atomically un-revokes the row. Without this,
    /// a user who re-authenticates would overwrite the ciphertext but stay
    /// flagged forever (`find_all_active_by_cli_tool` would keep skipping
    /// the row).
    pub async fn upsert_encrypted(&self, scope: &TenantScope, cli_tool: &str, ciphertext: &str) -> AppResult<()> {
        sqlx::query(
            r#"INSERT INTO user_cli_credentials (user_id, cli_tool, encrypted_credentials)
               VALUES ($1, $2, $3)
               ON CONFLICT (user_id, cli_tool) DO UPDATE
               SET encrypted_credentials = EXCLUDED.encrypted_credentials,
                   revoked_at = NULL,
                   revoke_reason = NULL,
                   refresh_fail_count = 0,
                   last_refresh_error = NULL,
                   last_refresh_error_at = NULL,
                   updated_at = NOW()"#,
        )
        .bind(scope.user_id().as_uuid())
        .bind(cli_tool)
        .bind(ciphertext)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Delete the stored blob for this user+tool. No-op if nothing is stored.
    pub async fn delete(&self, scope: &TenantScope, cli_tool: &str) -> AppResult<()> {
        sqlx::query(r#"DELETE FROM user_cli_credentials WHERE user_id = $1 AND cli_tool = $2"#)
            .bind(scope.user_id().as_uuid())
            .bind(cli_tool)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// All stored ciphertext rows for a given Container CLI across every user.
    /// Only the background refresh worker uses this — it has no caller-provided
    /// tenant scope because it sweeps every deployment user. Returns `(user_id,
    /// ciphertext)` pairs so the caller can pivot back into a per-user write via
    /// `upsert_encrypted`.
    pub async fn find_all_by_cli_tool(&self, cli_tool: &str) -> AppResult<Vec<(uuid::Uuid, String)>> {
        let rows: Vec<(uuid::Uuid, String)> =
            sqlx::query_as(r#"SELECT user_id, encrypted_credentials FROM user_cli_credentials WHERE cli_tool = $1"#)
                .bind(cli_tool)
                .fetch_all(&self.pool)
                .await?;
        Ok(rows)
    }

    /// Upsert keyed by explicit `user_id` instead of `TenantScope`. Used by
    /// the refresh worker when it has a `user_id` but not a full scope, and
    /// by the NATS credential-sync consumer.
    ///
    /// Clears revocation markers on the `DO UPDATE SET` path (see
    /// `upsert_encrypted` for rationale).
    pub async fn upsert_encrypted_by_user_id(
        &self,
        user_id: uuid::Uuid,
        cli_tool: &str,
        ciphertext: &str,
    ) -> AppResult<()> {
        sqlx::query(
            r#"INSERT INTO user_cli_credentials (user_id, cli_tool, encrypted_credentials)
               VALUES ($1, $2, $3)
               ON CONFLICT (user_id, cli_tool) DO UPDATE
               SET encrypted_credentials = EXCLUDED.encrypted_credentials,
                   revoked_at = NULL,
                   revoke_reason = NULL,
                   refresh_fail_count = 0,
                   last_refresh_error = NULL,
                   last_refresh_error_at = NULL,
                   updated_at = NOW()"#,
        )
        .bind(user_id)
        .bind(cli_tool)
        .bind(ciphertext)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Atomic bump-and-maybe-revoke for OAuth refresh failures.
    ///
    /// Increments `refresh_fail_count` by 1; if the post-increment value
    /// crosses `threshold`, sets `revoked_at = NOW()` + `revoke_reason =
    /// $reason` in the same UPDATE so concurrent sweeps can't race past the
    /// boundary (one worker bumps 0→1, another bumps 1→2 with `revoked_at`
    /// set atomically).
    ///
    /// Returns `Some((new_count, Some(revoked_at)))` when the threshold was
    /// just crossed, `Some((new_count, None))` when still below, or `None`
    /// when the row is already revoked or missing (caller has nothing to do
    /// either way).
    pub async fn bump_fail_count_or_revoke(
        &self,
        user_id: uuid::Uuid,
        cli_tool: &str,
        reason: &str,
        threshold: i32,
    ) -> AppResult<Option<(i32, Option<chrono::DateTime<chrono::Utc>>)>> {
        let row: Option<(i32, Option<chrono::DateTime<chrono::Utc>>)> = sqlx::query_as(
            r#"UPDATE user_cli_credentials
               SET refresh_fail_count = refresh_fail_count + 1,
                   last_refresh_error = $3,
                   last_refresh_error_at = NOW(),
                   revoked_at = CASE
                       WHEN refresh_fail_count + 1 >= $4 THEN NOW()
                       ELSE revoked_at
                   END,
                   revoke_reason = CASE
                       WHEN refresh_fail_count + 1 >= $4 THEN $3
                       ELSE revoke_reason
                   END,
                   updated_at = NOW()
               WHERE user_id = $1 AND cli_tool = $2 AND revoked_at IS NULL
               RETURNING refresh_fail_count, revoked_at"#,
        )
        .bind(user_id)
        .bind(cli_tool)
        .bind(reason)
        .bind(threshold)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    /// Reset fail counter + clear last error after a successful refresh.
    /// No-op on already-revoked rows (they won't be picked up by the sweep
    /// filter anyway — re-auth goes through `upsert_encrypted*` which
    /// clears all revocation state).
    pub async fn reset_fail_count_on_success(&self, user_id: uuid::Uuid, cli_tool: &str) -> AppResult<()> {
        sqlx::query(
            r#"UPDATE user_cli_credentials
               SET refresh_fail_count = 0,
                   last_refresh_error = NULL,
                   last_refresh_error_at = NULL,
                   updated_at = NOW()
               WHERE user_id = $1 AND cli_tool = $2 AND revoked_at IS NULL"#,
        )
        .bind(user_id)
        .bind(cli_tool)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Same as `find_all_by_cli_tool` but skips revoked rows. The refresh
    /// worker uses this so revoked rows aren't re-attempted every sweep.
    pub async fn find_all_active_by_cli_tool(&self, cli_tool: &str) -> AppResult<Vec<(uuid::Uuid, String)>> {
        let rows: Vec<(uuid::Uuid, String)> = sqlx::query_as(
            r#"SELECT user_id, encrypted_credentials
               FROM user_cli_credentials
               WHERE cli_tool = $1 AND revoked_at IS NULL"#,
        )
        .bind(cli_tool)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    /// Fetch ciphertext + revocation state in one query — the status
    /// endpoint uses this so it can surface `revoked_at` to the frontend.
    pub async fn find_encrypted_with_revocation(
        &self,
        scope: &TenantScope,
        cli_tool: &str,
    ) -> AppResult<Option<EncryptedWithRevocation>> {
        let row: Option<EncryptedWithRevocation> = sqlx::query_as(
            r#"SELECT encrypted_credentials, revoked_at, revoke_reason, refresh_fail_count
                   FROM user_cli_credentials
                   WHERE user_id = $1 AND cli_tool = $2"#,
        )
        .bind(scope.user_id().as_uuid())
        .bind(cli_tool)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    /// Runtime credential lookup — identical to `find_encrypted` but filters
    /// out revoked rows so new agent containers never receive ciphertext the
    /// refresh worker has already flagged as invalid.
    ///
    /// When the row exists but is revoked this returns `None`, letting
    /// `CliCredentialService::resolve` fall through to the system-wide
    /// fallback API-key tier.
    pub async fn find_encrypted_active(&self, scope: &TenantScope, cli_tool: &str) -> AppResult<Option<String>> {
        let row: Option<(String,)> = sqlx::query_as(
            r#"SELECT encrypted_credentials
               FROM user_cli_credentials
               WHERE user_id = $1 AND cli_tool = $2 AND revoked_at IS NULL"#,
        )
        .bind(scope.user_id().as_uuid())
        .bind(cli_tool)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|(s,)| s))
    }

    /// Summary of every stored Container CLI connection for this user (no
    /// ciphertext). Drives the "which Container CLIs am I logged into?" status
    /// card.
    pub async fn list_for_user(&self, scope: &TenantScope) -> AppResult<Vec<CliCredentialStatus>> {
        let rows: Vec<(String, chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>)> = sqlx::query_as(
            r#"SELECT cli_tool, created_at, updated_at FROM user_cli_credentials
               WHERE user_id = $1
               ORDER BY cli_tool"#,
        )
        .bind(scope.user_id().as_uuid())
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|(cli_tool, created_at, updated_at)| CliCredentialStatus { cli_tool, created_at, updated_at })
            .collect())
    }
}

/// Lightweight projection row for the list endpoint — ciphertext never leaves the DB.
#[derive(Debug, Clone, serde::Serialize)]
pub struct CliCredentialStatus {
    pub cli_tool: String,
    #[serde(rename = "createdAt")]
    pub created_at: chrono::DateTime<chrono::Utc>,
    #[serde(rename = "updatedAt")]
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

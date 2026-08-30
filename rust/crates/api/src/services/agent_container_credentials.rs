//! Agent container credential injection service.
//!
//! Owns the runtime credential orchestration needed while starting/stopping a
//! container-backed agent: sidecar credential-sync env, Container CLI
//! credentials, Git platform CLI env, and OAuth mount cleanup.

use agentforge_core::{AppConfig, AppResult, TenantScope};
use agentforge_db::entities::Agent;
use agentforge_platform::Mount;
use sqlx::PgPool;
use uuid::Uuid;

use crate::domain::agent::AgentContainerEnvPolicy;
use crate::domain::credential::ContainerCliCredentialPolicy;
use crate::repositories::credential::cli::CliCredentialRepository;
use crate::repositories::credential::git::GitCredentialRepository;
use crate::repositories::user::llm_config::UserLlmConfigRepository;
use crate::services::admin::PlatformAdminAuthority;
use crate::services::cli_credential::CliCredentialService;
use crate::services::git_credential::GitCredentialService;

pub(crate) struct AgentContainerCredentialService {
    cli_credentials: CliCredentialService,
    git_credentials: GitCredentialService,
    encryption_key: Option<[u8; 32]>,
    credential_sync_enabled: bool,
}

impl AgentContainerCredentialService {
    pub(crate) fn from_pool_and_app_config(pool: PgPool, encryption_key: Option<[u8; 32]>, config: &AppConfig) -> Self {
        Self::from_app_config(
            CliCredentialRepository::new(pool.clone()),
            UserLlmConfigRepository::new(pool.clone()),
            GitCredentialRepository::new(pool),
            encryption_key,
            config,
        )
    }

    pub(crate) fn from_app_config(
        cli_credentials: CliCredentialRepository,
        user_llm_configs: UserLlmConfigRepository,
        git_credentials: GitCredentialRepository,
        encryption_key: Option<[u8; 32]>,
        config: &AppConfig,
    ) -> Self {
        let cli_credentials =
            CliCredentialService::from_app_config(cli_credentials, user_llm_configs, encryption_key, config);
        let git_credentials = GitCredentialService::new(git_credentials);

        Self {
            cli_credentials,
            git_credentials,
            encryption_key,
            credential_sync_enabled: config.credential_sync_enabled,
        }
    }

    pub(crate) async fn inject_runtime_credentials(
        &self,
        scope: &TenantScope,
        agent_id: Uuid,
        cli_tool: Option<&str>,
        container_name: &str,
        env: &mut Vec<String>,
        mounts: &mut Vec<Mount>,
    ) -> AppResult<()> {
        self.inject_credential_sync_env(cli_tool, env);

        if let Some(cli_tool) = cli_tool {
            self.inject_container_cli_credentials(scope, cli_tool, container_name, env, mounts).await?;
        }

        self.inject_git_cli_credentials(scope, agent_id, env).await;
        Ok(())
    }

    /// Resolve credentials for the authoritative Agent row selected by the
    /// sealed platform-admin lifecycle coordinator. No tenant scope is forged:
    /// exact owner/org ids come from the row re-read under the Agent lock.
    pub(crate) async fn inject_runtime_credentials_as_platform_admin(
        &self,
        _authority: &PlatformAdminAuthority,
        agent: &Agent,
        container_name: &str,
        env: &mut Vec<String>,
        mounts: &mut Vec<Mount>,
    ) -> AppResult<()> {
        self.inject_credential_sync_env(agent.cli_tool.as_deref(), env);
        if let Some(cli_tool) = agent.cli_tool.as_deref() {
            let injection =
                self.cli_credentials.resolve_for_owner(agent.user_id.as_uuid(), cli_tool, container_name).await?;
            if injection.env.is_empty() && injection.oauth_mount_host_dir.is_none() {
                return Err(ContainerCliCredentialPolicy::runtime_credentials_required(cli_tool).into());
            }
            env.extend(injection.env.into_iter().map(|(key, value)| format!("{key}={value}")));
            if let Some(host_dir) = injection.oauth_mount_host_dir {
                mounts.push(Mount {
                    source: host_dir.to_string_lossy().into_owned(),
                    target: "/run/secrets/oauth-credentials".to_string(),
                    read_only: true,
                });
            }
        }
        match self
            .git_credentials
            .resolve_cli_env_for_owner(agent.organization_id.as_uuid(), agent.user_id.as_uuid(), self.encryption_key)
            .await
        {
            Ok(injection) => env.extend(injection.env.into_iter().map(|(key, value)| format!("{key}={value}"))),
            Err(err) => tracing::warn!(
                error = ?err,
                agent_id = %agent.id,
                "Failed to resolve Git platform CLI credentials - container will boot without gh/glab token injection"
            ),
        }
        Ok(())
    }

    pub(crate) async fn cleanup_oauth_mount_best_effort(&self, agent_id: Uuid, container_name: &str) {
        if let Err(err) = self.cli_credentials.cleanup_oauth_mount(container_name).await {
            tracing::warn!(
                error = %err,
                agent_id = %agent_id,
                "Failed to clean up OAuth mount dir - decrypted blob may linger on disk"
            );
        }
    }

    fn inject_credential_sync_env(&self, cli_tool: Option<&str>, env: &mut Vec<String>) {
        env.push(format!("CREDENTIAL_SYNC_ENABLED={}", self.credential_sync_enabled));
        if let Some(cli_tool) = cli_tool
            && let Some(dir) = AgentContainerEnvPolicy::creds_dir_for_cli_tool(cli_tool)
        {
            env.push(format!("CREDS_DIR={dir}"));
        }
    }

    async fn inject_container_cli_credentials(
        &self,
        scope: &TenantScope,
        cli_tool: &str,
        container_name: &str,
        env: &mut Vec<String>,
        mounts: &mut Vec<Mount>,
    ) -> AppResult<()> {
        let injection = self.cli_credentials.resolve(scope, cli_tool, container_name).await?;
        if injection.env.is_empty() && injection.oauth_mount_host_dir.is_none() {
            return Err(ContainerCliCredentialPolicy::runtime_credentials_required(cli_tool).into());
        }
        env.extend(injection.env.into_iter().map(|(key, value)| format!("{key}={value}")));
        if let Some(host_dir) = injection.oauth_mount_host_dir {
            mounts.push(Mount {
                source: host_dir.to_string_lossy().into_owned(),
                target: "/run/secrets/oauth-credentials".to_string(),
                read_only: true,
            });
        }
        Ok(())
    }

    async fn inject_git_cli_credentials(&self, scope: &TenantScope, agent_id: Uuid, env: &mut Vec<String>) {
        match self.git_credentials.resolve_cli_env(scope, self.encryption_key).await {
            Ok(injection) => {
                env.extend(injection.env.into_iter().map(|(key, value)| format!("{key}={value}")));
            }
            Err(err) => {
                tracing::warn!(
                    error = ?err,
                    agent_id = %agent_id,
                    "Failed to resolve Git platform CLI credentials - container will boot without gh/glab token injection"
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentforge_core::{ErrorKind, crypto};

    const TEST_KEY: [u8; 32] = [0x42; 32];

    async fn seed_user(pool: &PgPool) -> Uuid {
        let user_id = Uuid::new_v4();
        sqlx::query("INSERT INTO users (id, email) VALUES ($1, $2)")
            .bind(user_id)
            .bind(format!("credential-gate-{user_id}@example.com"))
            .execute(pool)
            .await
            .expect("seed user");
        user_id
    }

    fn service(pool: PgPool) -> AgentContainerCredentialService {
        let config = crate::test_support::test_app_config("postgres://localhost/agentforge_test");
        AgentContainerCredentialService::from_pool_and_app_config(pool, Some(TEST_KEY), &config)
    }

    #[sqlx::test(migrations = "../db/migrations")]
    async fn empty_cli_credentials_return_typed_actionable_error(pool: PgPool) {
        let user_id = seed_user(&pool).await;
        let scope = crate::test_support::tenant_scope_for_user(user_id);
        let mut env = Vec::new();
        let mut mounts = Vec::new();

        let err = service(pool)
            .inject_runtime_credentials(
                &scope,
                Uuid::new_v4(),
                Some("codex"),
                "agentforge-agent-empty",
                &mut env,
                &mut mounts,
            )
            .await
            .unwrap_err();

        match err.kind {
            ErrorKind::ValidationWithCode { code, message } => {
                assert_eq!(code, "errors.agent.lifecycle.cli_credentials_required");
                assert!(message.contains("Work tool sign-ins"));
            }
            other => panic!("expected typed credential rejection, got: {other:?}"),
        }
        assert!(mounts.is_empty());
        assert!(env.iter().all(|entry| !entry.starts_with("OPENAI_API_KEY=")));
    }

    #[sqlx::test(migrations = "../db/migrations")]
    async fn broken_cli_credentials_preserve_resolver_error(pool: PgPool) {
        let user_id = seed_user(&pool).await;
        let wrong_key = [0x99; 32];
        let encrypted = crypto::encrypt_base64(&wrong_key, "sk-invalid-after-key-rotation").unwrap();
        sqlx::query(
            "INSERT INTO user_llm_configs (user_id, provider, encrypted_api_key, is_default) \
             VALUES ($1, 'openai', $2, TRUE)",
        )
        .bind(user_id)
        .bind(encrypted)
        .execute(&pool)
        .await
        .unwrap();
        let scope = crate::test_support::tenant_scope_for_user(user_id);
        let mut env = Vec::new();
        let mut mounts = Vec::new();

        let err = service(pool)
            .inject_runtime_credentials(
                &scope,
                Uuid::new_v4(),
                Some("codex"),
                "agentforge-agent-broken",
                &mut env,
                &mut mounts,
            )
            .await
            .unwrap_err();

        assert!(matches!(err.kind, ErrorKind::Internal(_)));
        assert!(mounts.is_empty());
        assert!(env.iter().all(|entry| !entry.starts_with("OPENAI_API_KEY=")));
    }

    #[sqlx::test(migrations = "../db/migrations")]
    async fn broken_oauth_credentials_return_the_reconnect_action(pool: PgPool) {
        let user_id = seed_user(&pool).await;
        let wrong_key = [0x99; 32];
        let encrypted = crypto::encrypt_base64(&wrong_key, r#"{"auth.json":"{}"}"#).unwrap();
        sqlx::query(
            "INSERT INTO user_cli_credentials (user_id, cli_tool, encrypted_credentials) VALUES ($1, 'codex', $2)",
        )
        .bind(user_id)
        .bind(encrypted)
        .execute(&pool)
        .await
        .unwrap();
        let scope = crate::test_support::tenant_scope_for_user(user_id);
        let mut env = Vec::new();
        let mut mounts = Vec::new();

        let err = service(pool)
            .inject_runtime_credentials(
                &scope,
                Uuid::new_v4(),
                Some("codex"),
                "agentforge-agent-broken-oauth",
                &mut env,
                &mut mounts,
            )
            .await
            .unwrap_err();

        match err.kind {
            ErrorKind::ValidationWithCode { code, message } => {
                assert_eq!(code, "errors.agent.lifecycle.cli_credentials_required");
                assert!(message.contains("Work tool sign-ins"));
            }
            other => panic!("expected reconnect action, got: {other:?}"),
        }
        assert!(mounts.is_empty());
        assert!(env.iter().all(|entry| !entry.starts_with("OPENAI_API_KEY=")));
    }
}

//! Agent container credential injection service.
//!
//! Owns the runtime credential orchestration needed while starting/stopping a
//! container-backed agent: sidecar credential-sync env, Container CLI
//! credentials, Git platform CLI env, and OAuth mount cleanup.

use std::path::PathBuf;

use agentforge_core::TenantScope;
use agentforge_platform::Mount;
use secrecy::{ExposeSecret, SecretString};
use uuid::Uuid;

use crate::domain::agent::AgentContainerEnvPolicy;
use crate::repositories::credential::cli::CliCredentialRepository;
use crate::repositories::credential::git::GitCredentialRepository;
use crate::repositories::user::llm_config::UserLlmConfigRepository;
use crate::services::cli_credential::CliCredentialService;
use crate::services::git_credential::GitCredentialService;

pub(crate) struct AgentContainerCredentialService {
    cli_credentials: CliCredentialService,
    git_credentials: GitCredentialService,
    encryption_key: Option<[u8; 32]>,
    credential_sync_enabled: bool,
}

impl AgentContainerCredentialService {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        cli_credentials: CliCredentialRepository,
        user_llm_configs: UserLlmConfigRepository,
        git_credentials: GitCredentialRepository,
        encryption_key: Option<[u8; 32]>,
        oauth_mount_root: PathBuf,
        credential_sync_enabled: bool,
        system_anthropic: &Option<SecretString>,
        system_google: &Option<SecretString>,
        system_openai: &Option<SecretString>,
    ) -> Self {
        let cli_credentials = CliCredentialService::new(
            cli_credentials,
            user_llm_configs,
            encryption_key,
            oauth_mount_root,
            clone_secret(system_anthropic),
            clone_secret(system_google),
            clone_secret(system_openai),
        );
        let git_credentials = GitCredentialService::new(git_credentials);

        Self { cli_credentials, git_credentials, encryption_key, credential_sync_enabled }
    }

    pub(crate) async fn inject_runtime_credentials(
        &self,
        scope: &TenantScope,
        agent_id: Uuid,
        cli_tool: Option<&str>,
        container_name: &str,
        env: &mut Vec<String>,
        mounts: &mut Vec<Mount>,
    ) {
        self.inject_credential_sync_env(cli_tool, env);

        if let Some(cli_tool) = cli_tool {
            self.inject_container_cli_credentials(scope, agent_id, cli_tool, container_name, env, mounts).await;
        }

        self.inject_git_cli_credentials(scope, agent_id, env).await;
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
        agent_id: Uuid,
        cli_tool: &str,
        container_name: &str,
        env: &mut Vec<String>,
        mounts: &mut Vec<Mount>,
    ) {
        match self.cli_credentials.resolve(scope, cli_tool, container_name).await {
            Ok(injection) => {
                env.extend(injection.env.into_iter().map(|(key, value)| format!("{key}={value}")));
                if let Some(host_dir) = injection.oauth_mount_host_dir {
                    mounts.push(Mount {
                        source: host_dir.to_string_lossy().into_owned(),
                        target: "/run/secrets/oauth-credentials".to_string(),
                        read_only: true,
                    });
                }
            }
            Err(err) => {
                tracing::warn!(
                    error = ?err,
                    agent_id = %agent_id,
                    cli_tool,
                    "Failed to resolve Container CLI credentials - container will boot without injected auth"
                );
            }
        }
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

fn clone_secret(secret: &Option<SecretString>) -> Option<SecretString> {
    secret.as_ref().map(|value| SecretString::from(value.expose_secret().to_string()))
}

//! Host CLI agent enrollment.
//!
//! This path creates a normal managed agent row, issues the same per-agent
//! NATS/HMAC material used by container sidecars, and returns a one-time shell
//! environment for running a sidecar on an operator-managed machine.
//!
//! Key invariants enforced here (see spec §6.3):
//! - TLS gate: non-`tls://` NATS URLs are rejected unless `allow_plaintext_host_nats` is set.
//! - Idempotency fast path: the same `(org_id, user_id, idempotency_key)` triple
//!   returns the original response without creating a second agent.
//! - Atomic cold path: agent INSERT and idempotency record are written in a
//!   single transaction via `create_aggregate_in_tx`.

use std::collections::BTreeMap;

use agentforge_core::{AgentId, AppConfig, AppError, AppResult, CliToolKind, RuntimeKind, TenantScope};
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::domain::agent::{
    AgentContainerEnvInput, AgentContainerEnvPolicy, AgentName, ClaimedHostAgentJoin, EnrolledHostCli,
    HostAgentEnrollment, HostAgentEnrollmentPolicy, HostCliIdentity, JoinCode, NewAgent,
};
pub(crate) use crate::domain::agent::agent_join_claim_response;
use crate::domain::context::{ContextFeature, ContextFeatureFlags};
use crate::repositories::agent::{AgentJoinCodeRepository, AgentListItem, AgentRepository};
use crate::repositories::enrollment_idempotency::EnrollmentIdempotencyRepository;
use crate::services::agent_workspace::AgentWorkspaceService;

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct HostAgentEnrollmentInput<'a> {
    pub(crate) name: Option<&'a str>,
    pub(crate) model: Option<&'a str>,
    pub(crate) cli_tool: &'a str,
    pub(crate) cwd: Option<&'a str>,
    pub(crate) workspace_id: Option<Uuid>,
    pub(crate) project_id: Option<Uuid>,
}

#[derive(Debug, Clone)]
struct HostAgentEnrollmentSettings {
    nats_agent_url: Option<String>,
    nats_url: Option<String>,
    server_url: Option<String>,
    codex_default_model: String,
    context_injection_enabled: bool,
    allow_plaintext_host_nats: bool,
}

pub(crate) struct HostAgentEnrollmentService {
    pool: PgPool,
    agents: AgentRepository,
    idempotency: EnrollmentIdempotencyRepository,
    join_codes: AgentJoinCodeRepository,
    workspaces: AgentWorkspaceService,
    settings: HostAgentEnrollmentSettings,
}

impl HostAgentEnrollmentService {
    pub(crate) fn from_runtime(pool: PgPool, config: &AppConfig, context_features: ContextFeatureFlags) -> Self {
        Self {
            agents: AgentRepository::new(pool.clone()),
            idempotency: EnrollmentIdempotencyRepository::new(pool.clone()),
            join_codes: AgentJoinCodeRepository::new(pool.clone()),
            workspaces: AgentWorkspaceService::from_pool(pool.clone()),
            settings: HostAgentEnrollmentSettings {
                nats_agent_url: config.nats_agent_url.clone(),
                nats_url: config.nats_url.clone(),
                server_url: config.app_url.clone().or_else(|| config.container_server_url.clone()),
                codex_default_model: config.codex_default_model.clone(),
                context_injection_enabled: context_features.enabled(ContextFeature::Injection),
                allow_plaintext_host_nats: config.allow_plaintext_host_nats,
            },
            pool,
        }
    }

    pub(crate) async fn enroll(
        &self,
        scope: &TenantScope,
        idempotency_key: &str,
        input: HostAgentEnrollmentInput<'_>,
    ) -> AppResult<(AgentListItem, HostAgentEnrollment)> {
        // 1. Validate name + cli_tool + NATS URL.
        AgentName::validate(input.name)?;
        let cli_tool_str = HostAgentEnrollmentPolicy::require_cli_tool(input.cli_tool)?;
        let nats_base_url = HostAgentEnrollmentPolicy::require_nats_base_url(
            self.settings.nats_agent_url.as_deref(),
            self.settings.nats_url.as_deref(),
        )?;

        // 2. TLS gate.
        if !nats_base_url.starts_with("tls://") && !self.settings.allow_plaintext_host_nats {
            return Err(HostAgentEnrollmentPolicy::plaintext_nats_blocked_error());
        }

        let org_id = scope.org_id().as_uuid();
        let user_id = scope.user_id().as_uuid();

        // 3. Idempotency fast path. The original join code is never stored in
        //    plaintext, so a replay mints a fresh one (older codes stay valid
        //    until their own expiry).
        if let Some(existing_id) = self.idempotency.lookup(org_id, user_id, idempotency_key).await? {
            metrics::counter!("agents_idempotency_replay_total").increment(1);
            let agent = self.agents.find_with_owner_by_id(scope, AgentId::from(existing_id)).await?;
            let mut enrollment = self.rebuild_enrollment_view(scope, &agent, existing_id, &nats_base_url).await?;
            let (code, expires_at) = (JoinCode::generate(), JoinCode::expires_at_from(Utc::now()));
            self.join_codes.store(org_id, existing_id, &code.hash_hex(), expires_at).await?;
            self.attach_join_code(&mut enrollment, &code, expires_at);
            return Ok((agent, enrollment));
        }

        // 4. Cold path.
        let workspace_scope =
            self.workspaces.resolve_workspace_mount_scope(org_id, input.workspace_id, input.project_id).await?;

        let identity = HostCliIdentity::generate();
        let cli_kind = CliToolKind::parse_legacy(cli_tool_str)
            .map_err(|_| HostAgentEnrollmentPolicy::unknown_cli_tool_error(cli_tool_str))?;
        let new_agent = NewAgent::host_cli(
            scope,
            cli_kind,
            identity.clone(),
            input.name,
            input.model,
            input.cwd,
            workspace_scope.workspace_id,
            input.project_id,
        )?;

        let (code, code_expires_at) = (JoinCode::generate(), JoinCode::expires_at_from(Utc::now()));
        let mut tx = self.pool.begin().await.map_err(AppError::from)?;
        let id = self.agents.create_aggregate_in_tx(&mut tx, scope, new_agent).await?;
        EnrollmentIdempotencyRepository::store_in_tx(&mut tx, org_id, user_id, idempotency_key, id).await?;
        AgentJoinCodeRepository::store_in_tx(&mut tx, org_id, id, &code.hash_hex(), code_expires_at).await?;
        tx.commit().await.map_err(AppError::from)?;

        metrics::counter!(
            "agents_enrolled_total",
            "cli_tool" => cli_tool_str.to_string()
        )
        .increment(1);

        let agent = self.agents.find_with_owner_by_id(scope, AgentId::from(id)).await?;
        let mut enrollment = self.build_enrollment_view(&agent, &identity, &nats_base_url);
        self.attach_join_code(&mut enrollment, &code, code_expires_at);
        Ok((agent, enrollment))
    }

    /// Decorate an enrollment view with the freshly minted pairing code and,
    /// when the deployment knows its public URL, the copy-paste join commands.
    fn attach_join_code(&self, enrollment: &mut HostAgentEnrollment, code: &JoinCode, expires_at: DateTime<Utc>) {
        enrollment.join_code = Some(code.as_str().to_string());
        enrollment.join_code_expires_at = Some(expires_at);
        if let Some(server_url) = self.settings.server_url.as_deref() {
            enrollment.join_command = Some(HostAgentEnrollmentPolicy::join_command(server_url, code.as_str()));
            enrollment.join_command_powershell =
                Some(HostAgentEnrollmentPolicy::join_command_powershell(server_url, code.as_str()));
        }
    }

    /// Exchange a pairing code for the agent's sidecar environment.
    ///
    /// Unauthenticated by design: the code is the credential. Unknown,
    /// expired, malformed, and non-host-CLI codes all collapse into one
    /// opaque rejection so the endpoint cannot be used as an oracle. The
    /// enrollment TLS gate is re-checked because the claim re-derives the
    /// NATS URL from current settings.
    pub(crate) async fn claim(&self, raw_code: &str) -> AppResult<ClaimedHostAgentJoin> {
        let code = JoinCode::parse(raw_code)?;
        let nats_base_url = HostAgentEnrollmentPolicy::require_nats_base_url(
            self.settings.nats_agent_url.as_deref(),
            self.settings.nats_url.as_deref(),
        )?;
        if !nats_base_url.starts_with("tls://") && !self.settings.allow_plaintext_host_nats {
            return Err(HostAgentEnrollmentPolicy::plaintext_nats_blocked_error());
        }

        let Some(row) = self.join_codes.find_valid_claim(&code.hash_hex()).await? else {
            metrics::counter!("agents_join_claim_rejected_total").increment(1);
            return Err(HostAgentEnrollmentPolicy::invalid_join_code_error());
        };
        if row.runtime_kind != RuntimeKind::Cli.as_str() {
            metrics::counter!("agents_join_claim_rejected_total").increment(1);
            return Err(HostAgentEnrollmentPolicy::invalid_join_code_error());
        }
        let (Some(hmac_secret), Some(nats_connect_password)) =
            (row.hmac_secret.as_deref(), row.nats_connect_password.as_deref())
        else {
            metrics::counter!("agents_join_claim_rejected_total").increment(1);
            return Err(HostAgentEnrollmentPolicy::invalid_join_code_error());
        };

        self.join_codes.record_claim(row.join_code_id).await?;
        metrics::counter!("agents_join_claimed_total").increment(1);

        let env = AgentContainerEnvPolicy::build(AgentContainerEnvInput {
            agent_id: row.agent_id,
            org_id: row.organization_id,
            cli_tool: row.cli_tool.as_deref(),
            cli_model: row.model.as_deref(),
            codex_default_model: Some(self.settings.codex_default_model.as_str()),
            nats_base_url: Some(&nats_base_url),
            nats_connect_password,
            container_server_url: self.settings.server_url.as_deref(),
            workspace_host_path: None,
            hmac_secret,
            context_injection_enabled: self.settings.context_injection_enabled,
        });
        let mut env_map: BTreeMap<String, String> = HostAgentEnrollmentPolicy::env_map(env);
        env_map.insert("AGENTFORGE_RUNTIME_KIND".to_string(), "cli".to_string());

        Ok(ClaimedHostAgentJoin {
            agent_id: row.agent_id,
            agent_name: row.agent_name.clone(),
            cli_tool: row.cli_tool.clone(),
            shell_export_lines: HostAgentEnrollmentPolicy::shell_export_lines(&env_map),
            powershell_export_lines: HostAgentEnrollmentPolicy::powershell_export_lines(&env_map),
            sidecar_command: HostAgentEnrollmentPolicy::SIDECAR_COMMAND.to_string(),
            env: env_map,
        })
    }

    fn build_enrollment_view(
        &self,
        agent: &AgentListItem,
        identity: &HostCliIdentity,
        nats_base_url: &str,
    ) -> HostAgentEnrollment {
        let env = AgentContainerEnvPolicy::build(AgentContainerEnvInput {
            agent_id: agent.id,
            org_id: agent.organization_id,
            cli_tool: agent.cli_tool.as_deref(),
            cli_model: agent.model.as_deref(),
            codex_default_model: Some(self.settings.codex_default_model.as_str()),
            nats_base_url: Some(nats_base_url),
            nats_connect_password: identity.nats_connect_password(),
            container_server_url: self.settings.server_url.as_deref(),
            workspace_host_path: None,
            hmac_secret: identity.hmac_secret(),
            context_injection_enabled: self.settings.context_injection_enabled,
        });
        let mut env_map: BTreeMap<String, String> = HostAgentEnrollmentPolicy::env_map(env);
        env_map.insert("AGENTFORGE_RUNTIME_KIND".to_string(), "cli".to_string());
        let shell_exports = HostAgentEnrollmentPolicy::shell_exports(&env_map);
        HostAgentEnrollment {
            agent_id: agent.id,
            runtime_id: identity.runtime_id().to_string(),
            cli_tool: agent.cli_tool.clone().unwrap_or_default(),
            env: env_map,
            shell_exports,
            sidecar_command: HostAgentEnrollmentPolicy::SIDECAR_COMMAND.to_string(),
            server_url: self.settings.server_url.clone(),
            join_code: None,
            join_code_expires_at: None,
            join_command: None,
            join_command_powershell: None,
        }
    }

    /// Rebuild the enrollment env from a previously created agent row.
    ///
    /// Called on idempotent replay: the agent already exists and we must return
    /// the same credentials the operator received during the original enrollment.
    /// `hmac_secret` and `nats_connect_password` are stored on the agent row
    /// (not in `AgentListItem` to avoid accidental serialization); this method
    /// fetches them and reassembles the env block.
    ///
    /// Credential issuance is gated by the [`EnrolledHostCli`] typestate: the
    /// aggregate is loaded and validated to be a host-CLI runtime (with its
    /// `runtime_id` present) BEFORE any NATS/HMAC material is fetched. A
    /// container/api agent reaching this path (already wrong — replay only keys
    /// off enrollment idempotency records, which are host-CLI) is now a typed
    /// rejection instead of a NULL-credential fetch. The validated `runtime_id`
    /// comes from the typestate, replacing the nullable-column re-read.
    async fn rebuild_enrollment_view(
        &self,
        scope: &TenantScope,
        agent: &AgentListItem,
        id: Uuid,
        nats_base_url: &str,
    ) -> AppResult<HostAgentEnrollment> {
        let aggregate = self.agents.find_aggregate(scope, id).await?;
        let enrolled = EnrolledHostCli::try_from(aggregate).map_err(|rejection| {
            metrics::counter!(
                "agents_host_cli_credential_rejected_total",
                "runtime_kind" => agent.runtime_kind.as_str()
            )
            .increment(1);
            rejection.into_app_error()
        })?;
        let runtime_id = enrolled.runtime_id().to_string();

        let (hmac_secret, nats_connect_password) =
            self.agents.fetch_host_cli_credentials(scope, enrolled.inner().id).await?;

        let env = AgentContainerEnvPolicy::build(AgentContainerEnvInput {
            agent_id: agent.id,
            org_id: agent.organization_id,
            cli_tool: agent.cli_tool.as_deref(),
            cli_model: agent.model.as_deref(),
            codex_default_model: Some(self.settings.codex_default_model.as_str()),
            nats_base_url: Some(nats_base_url),
            nats_connect_password: &nats_connect_password,
            container_server_url: self.settings.server_url.as_deref(),
            workspace_host_path: None,
            hmac_secret: &hmac_secret,
            context_injection_enabled: self.settings.context_injection_enabled,
        });
        let mut env_map: BTreeMap<String, String> = HostAgentEnrollmentPolicy::env_map(env);
        env_map.insert("AGENTFORGE_RUNTIME_KIND".to_string(), "cli".to_string());
        let shell_exports = HostAgentEnrollmentPolicy::shell_exports(&env_map);
        let cli_tool = agent.cli_tool.clone().unwrap_or_default();
        Ok(HostAgentEnrollment {
            agent_id: agent.id,
            runtime_id,
            cli_tool,
            env: env_map,
            shell_exports,
            sidecar_command: HostAgentEnrollmentPolicy::SIDECAR_COMMAND.to_string(),
            server_url: self.settings.server_url.clone(),
            join_code: None,
            join_code_expires_at: None,
            join_command: None,
            join_command_powershell: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::agent::{AgentRuntime, EnrolledHostCli, HostCliRejection};
    use agentforge_core::ErrorKind;

    /// Seed a minimal (organization + workspace + user) triple.
    /// Uses workspace_id == org_id (the same UUID trick used project-wide).
    /// Returns (org_id, workspace_id, user_id).
    async fn seed_org_workspace_user(pool: &PgPool) -> (Uuid, Uuid, Uuid) {
        let org_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();

        sqlx::query("INSERT INTO organizations (id, name, slug) VALUES ($1, $2, $3)")
            .bind(org_id)
            .bind(format!("Test Org {org_id}"))
            .bind(format!("org-{org_id}"))
            .execute(pool)
            .await
            .expect("seed organization");
        sqlx::query("INSERT INTO workspaces (id, organization_id, name) VALUES ($1, $1, 'Default')")
            .bind(org_id)
            .execute(pool)
            .await
            .expect("seed workspace");
        sqlx::query("INSERT INTO users (id, email) VALUES ($1, $2)")
            .bind(user_id)
            .bind(format!("u-{user_id}@example.com"))
            .execute(pool)
            .await
            .expect("seed user");

        (org_id, org_id, user_id)
    }

    /// Build the enrollment service directly from its fields with a `tls://`
    /// NATS URL so the TLS gate is satisfied without `allow_plaintext_host_nats`.
    fn service_with_tls_nats(pool: PgPool) -> HostAgentEnrollmentService {
        HostAgentEnrollmentService {
            agents: AgentRepository::new(pool.clone()),
            idempotency: EnrollmentIdempotencyRepository::new(pool.clone()),
            join_codes: AgentJoinCodeRepository::new(pool.clone()),
            workspaces: AgentWorkspaceService::from_pool(pool.clone()),
            settings: HostAgentEnrollmentSettings {
                nats_agent_url: Some("tls://nats:4222".to_string()),
                nats_url: None,
                server_url: Some("https://agentforge.example".to_string()),
                codex_default_model: "gpt-5.5".to_string(),
                context_injection_enabled: false,
                allow_plaintext_host_nats: false,
            },
            pool,
        }
    }

    /// Parity: enrolling a host-cli agent then replaying the same idempotency
    /// key returns the SAME agent id, runtime_id, and credential-bearing env.
    /// The replay path runs through the `EnrolledHostCli` typestate gate.
    #[sqlx::test(migrations = "../db/migrations")]
    async fn host_cli_replay_returns_identical_agent_and_creds(pool: PgPool) {
        let (org_id, _ws, user_id) = seed_org_workspace_user(&pool).await;
        let scope = crate::test_support::tenant_scope_for_ids(org_id, user_id);
        let svc = service_with_tls_nats(pool);
        let input = HostAgentEnrollmentInput { cli_tool: "codex", ..Default::default() };

        let (cold_agent, cold_view) = svc.enroll(&scope, "key-parity", input).await.expect("cold enroll");
        let (replay_agent, replay_view) = svc.enroll(&scope, "key-parity", input).await.expect("replay enroll");

        // Same agent, same runtime_id, same credential-bearing NATS env line.
        assert_eq!(cold_agent.id, replay_agent.id, "replay must not create a new agent");
        assert_eq!(cold_view.runtime_id, replay_view.runtime_id, "runtime_id parity");
        assert_eq!(cold_view.env.get("NATS_URL"), replay_view.env.get("NATS_URL"), "credential parity");
        assert!(
            replay_view.env.get("NATS_URL").is_some_and(|u| u.starts_with("tls://")),
            "replay env carries the per-agent tls NATS url, got {:?}",
            replay_view.env.get("NATS_URL")
        );
    }

    /// Defense-in-depth: if a NON-cli agent is reachable via an idempotency
    /// record (already wrong — replay only ever keys off host-cli enrollments),
    /// the replay path yields the typed `EnrolledHostCli` rejection (a 422
    /// ValidationWithCode), NOT a NULL-credential fetch.
    #[sqlx::test(migrations = "../db/migrations")]
    async fn replay_against_non_cli_agent_is_typed_rejection_not_null_creds(pool: PgPool) {
        let (org_id, ws, user_id) = seed_org_workspace_user(&pool).await;
        let scope = crate::test_support::tenant_scope_for_ids(org_id, user_id);
        let repo = AgentRepository::new(pool.clone());

        // Create a CONTAINER agent (no NATS credential columns).
        let container =
            NewAgent::container(&scope, CliToolKind::Codex, Some("ctr"), None, None, ws, None, None).unwrap();
        let container_id = repo.create_aggregate(&scope, container).await.expect("create container agent");

        // Point an idempotency record at the container agent (simulating drift).
        let mut tx = pool.begin().await.unwrap();
        EnrollmentIdempotencyRepository::store_in_tx(&mut tx, org_id, user_id, "key-bad", container_id)
            .await
            .expect("store idempotency record");
        tx.commit().await.unwrap();

        let svc = service_with_tls_nats(pool.clone());
        let input = HostAgentEnrollmentInput { cli_tool: "codex", ..Default::default() };
        let err = svc.enroll(&scope, "key-bad", input).await.expect_err("non-cli replay must be rejected");

        // Typed rejection (422 ValidationWithCode), not a NULL-credential 500.
        match &err.kind {
            ErrorKind::ValidationWithCode { code, .. } => {
                assert_eq!(*code, "errors.agent.enroll.not_host_cli_container");
            }
            other => panic!("expected ValidationWithCode for container agent, got: {other:?}"),
        }

        // And the typestate gate itself rejects the loaded aggregate directly.
        let aggregate = repo.find_aggregate(&scope, container_id).await.expect("load aggregate");
        assert!(matches!(aggregate.runtime().unwrap(), AgentRuntime::Container { .. }));
        match EnrolledHostCli::try_from(aggregate) {
            Err(HostCliRejection::Container) => (),
            other => panic!("expected Container rejection from typestate, got {other:?}"),
        }
    }

    /// The typestate gate accepts a real host-cli aggregate loaded from the DB
    /// and credentials can then be fetched for the validated inner id.
    #[sqlx::test(migrations = "../db/migrations")]
    async fn typestate_accepts_db_loaded_host_cli_and_fetches_creds(pool: PgPool) {
        let (org_id, ws, user_id) = seed_org_workspace_user(&pool).await;
        let scope = crate::test_support::tenant_scope_for_ids(org_id, user_id);
        let repo = AgentRepository::new(pool.clone());

        let identity = HostCliIdentity::generate();
        let expected_runtime_id = identity.runtime_id().to_string();
        let new_agent =
            NewAgent::host_cli(&scope, CliToolKind::Claude, identity, Some("host"), None, None, ws, None).unwrap();
        let id = repo.create_aggregate(&scope, new_agent).await.expect("create host-cli agent");

        let aggregate = repo.find_aggregate(&scope, id).await.expect("load aggregate");
        let enrolled = EnrolledHostCli::try_from(aggregate).expect("host-cli accepted by typestate");
        assert_eq!(enrolled.runtime_id(), expected_runtime_id, "runtime_id from validated variant");

        // Credentials are present for the validated inner id (no NULL fetch).
        let (hmac, nats_pw) =
            repo.fetch_host_cli_credentials(&scope, enrolled.inner().id).await.expect("fetch creds for host-cli");
        assert!(!hmac.is_empty(), "hmac_secret present");
        assert!(!nats_pw.is_empty(), "nats_connect_password present");
    }

    /// Enrollment mints a pairing code and, when the server URL is known,
    /// both copy-paste join commands embedding that code.
    #[sqlx::test(migrations = "../db/migrations")]
    async fn enroll_mints_join_code_and_commands(pool: PgPool) {
        let (org_id, _ws, user_id) = seed_org_workspace_user(&pool).await;
        let scope = crate::test_support::tenant_scope_for_ids(org_id, user_id);
        let svc = service_with_tls_nats(pool);
        let input = HostAgentEnrollmentInput { cli_tool: "claude", ..Default::default() };

        let (_agent, enrollment) = svc.enroll(&scope, "key-join", input).await.expect("enroll");

        let code = enrollment.join_code.as_deref().expect("join code minted");
        assert!(code.starts_with("afj_"), "code carries scanning prefix, got {code}");
        assert!(enrollment.join_code_expires_at.is_some(), "expiry returned");
        let cmd = enrollment.join_command.as_deref().expect("join command present");
        assert!(
            cmd.contains("https://agentforge.example/api/v1/agents/local-join/script") && cmd.contains(code),
            "bash command embeds server + code: {cmd}"
        );
        let ps = enrollment.join_command_powershell.as_deref().expect("powershell command present");
        assert!(ps.contains("script.ps1") && ps.contains(code), "powershell command embeds code: {ps}");
    }

    /// Claiming the minted code returns the same env the enrollment produced,
    /// plus eval-ready export lines for both shells.
    #[sqlx::test(migrations = "../db/migrations")]
    async fn claim_returns_enrollment_env_parity(pool: PgPool) {
        let (org_id, _ws, user_id) = seed_org_workspace_user(&pool).await;
        let scope = crate::test_support::tenant_scope_for_ids(org_id, user_id);
        let svc = service_with_tls_nats(pool);
        let input = HostAgentEnrollmentInput { cli_tool: "claude", ..Default::default() };

        let (agent, enrollment) = svc.enroll(&scope, "key-claim", input).await.expect("enroll");
        let code = enrollment.join_code.as_deref().expect("join code");

        let claimed = svc.claim(code).await.expect("claim succeeds");
        assert_eq!(claimed.agent_id, agent.id);
        assert_eq!(claimed.env, enrollment.env, "claimed env must match the enrollment env");
        assert!(claimed.shell_export_lines.contains("export NATS_URL="), "bash exports carry NATS creds");
        assert!(claimed.powershell_export_lines.contains("$env:NATS_URL ="), "powershell exports carry NATS creds");
        assert!(!claimed.shell_export_lines.contains(HostAgentEnrollmentPolicy::SIDECAR_COMMAND));

        // Codes stay claimable until expiry (interrupted bootstrap can retry).
        let again = svc.claim(code).await.expect("re-claim within TTL");
        assert_eq!(again.env, claimed.env);
    }

    /// Unknown, malformed, and expired codes all collapse into one opaque
    /// NotFound — the public endpoint must not act as an oracle.
    #[sqlx::test(migrations = "../db/migrations")]
    async fn claim_rejects_unknown_malformed_and_expired_codes(pool: PgPool) {
        let (org_id, _ws, user_id) = seed_org_workspace_user(&pool).await;
        let scope = crate::test_support::tenant_scope_for_ids(org_id, user_id);
        let svc = service_with_tls_nats(pool.clone());

        // Unknown (well-formed) code.
        let unknown = format!("afj_{}", "a".repeat(43));
        let err = svc.claim(&unknown).await.expect_err("unknown code rejected");
        assert!(matches!(err.kind, ErrorKind::NotFound(_)), "unknown → NotFound, got {:?}", err.kind);

        // Malformed code: same opaque shape.
        let err = svc.claim("not-a-code").await.expect_err("malformed code rejected");
        assert!(matches!(err.kind, ErrorKind::NotFound(_)), "malformed → NotFound, got {:?}", err.kind);

        // Expired code.
        let input = HostAgentEnrollmentInput { cli_tool: "claude", ..Default::default() };
        let (_agent, enrollment) = svc.enroll(&scope, "key-expire", input).await.expect("enroll");
        let code = enrollment.join_code.as_deref().expect("join code");
        sqlx::query("UPDATE agent_join_codes SET expires_at = NOW() - INTERVAL '1 minute'")
            .execute(&pool)
            .await
            .expect("expire codes");
        let err = svc.claim(code).await.expect_err("expired code rejected");
        assert!(matches!(err.kind, ErrorKind::NotFound(_)), "expired → NotFound, got {:?}", err.kind);
    }

    /// Idempotent replay returns the same agent but a FRESH pairing code —
    /// the original plaintext is never stored so it cannot be re-shown.
    #[sqlx::test(migrations = "../db/migrations")]
    async fn replay_mints_fresh_join_code_for_same_agent(pool: PgPool) {
        let (org_id, _ws, user_id) = seed_org_workspace_user(&pool).await;
        let scope = crate::test_support::tenant_scope_for_ids(org_id, user_id);
        let svc = service_with_tls_nats(pool);
        let input = HostAgentEnrollmentInput { cli_tool: "codex", ..Default::default() };

        let (cold_agent, cold) = svc.enroll(&scope, "key-fresh", input).await.expect("cold enroll");
        let (replay_agent, replay) = svc.enroll(&scope, "key-fresh", input).await.expect("replay enroll");

        assert_eq!(cold_agent.id, replay_agent.id);
        let cold_code = cold.join_code.as_deref().expect("cold code");
        let replay_code = replay.join_code.as_deref().expect("replay code");
        assert_ne!(cold_code, replay_code, "replay mints a fresh code");

        // Both codes resolve to the same agent.
        assert_eq!(svc.claim(cold_code).await.expect("cold claim").agent_id, cold_agent.id);
        assert_eq!(svc.claim(replay_code).await.expect("replay claim").agent_id, cold_agent.id);
    }
}

//! Dev environment service — lifecycle management.

use std::collections::HashMap;
use std::sync::Arc;

use agentforge_core::{AppError, AppResult, ErrorKind, TenantScope};
use agentforge_db::entities::DevEnvironment;
use agentforge_platform::DockerClient;
use agentforge_platform::types::{ContainerConfig, ContainerState, Mount, ResourceLimits};
use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

use crate::domain::dev_environment::{
    DEFAULT_STOP_TIMEOUT_SECONDS, DevEnvironmentLifecyclePolicy, DevEnvironmentName, DevEnvironmentRuntimeSpec,
    DevEnvironmentRuntimeState, DevEnvironmentStatusUpdate, ERROR_STATUS, RUNNING_STATUS, STARTING_STATUS,
    STOPPED_STATUS, StopPlan,
};
pub(crate) use crate::domain::dev_environment::{
    dev_environment_data_response, dev_environment_delete_response, dev_environment_message_response,
};
use crate::repositories::dev_environment::DevEnvironmentRepository;

/// Storage operations used by the dev environment service.
#[async_trait]
pub trait DevEnvironmentStore: Send + Sync {
    async fn list(&self, scope: &TenantScope) -> AppResult<Vec<DevEnvironment>>;
    async fn get(&self, scope: &TenantScope, id: Uuid) -> AppResult<DevEnvironment>;
    async fn create(
        &self,
        scope: &TenantScope,
        name: &str,
        project_id: Option<Uuid>,
        config: &serde_json::Value,
    ) -> AppResult<DevEnvironment>;
    async fn update_status(
        &self,
        scope: &TenantScope,
        id: Uuid,
        status: &str,
        container_id: Option<&str>,
    ) -> AppResult<DevEnvironment>;
    async fn delete(&self, scope: &TenantScope, id: Uuid) -> AppResult<()>;
}

#[async_trait]
impl DevEnvironmentStore for DevEnvironmentRepository {
    async fn list(&self, scope: &TenantScope) -> AppResult<Vec<DevEnvironment>> {
        DevEnvironmentRepository::list(self, scope).await
    }

    async fn get(&self, scope: &TenantScope, id: Uuid) -> AppResult<DevEnvironment> {
        DevEnvironmentRepository::get(self, scope, id).await
    }

    async fn create(
        &self,
        scope: &TenantScope,
        name: &str,
        project_id: Option<Uuid>,
        config: &serde_json::Value,
    ) -> AppResult<DevEnvironment> {
        DevEnvironmentRepository::create(self, scope, name, project_id, config).await
    }

    async fn update_status(
        &self,
        scope: &TenantScope,
        id: Uuid,
        status: &str,
        container_id: Option<&str>,
    ) -> AppResult<DevEnvironment> {
        DevEnvironmentRepository::update_status(self, scope, id, status, container_id).await
    }

    async fn delete(&self, scope: &TenantScope, id: Uuid) -> AppResult<()> {
        DevEnvironmentRepository::delete(self, scope, id).await
    }
}

/// Runtime operations needed to provision dev environment containers.
#[async_trait]
pub trait DevEnvironmentRuntime: Send + Sync {
    async fn create_container(&self, config: ContainerConfig) -> AppResult<String>;
    async fn start_container(&self, container_id: &str) -> AppResult<()>;
    async fn stop_container(&self, container_id: &str, timeout_secs: i64) -> AppResult<()>;
    async fn remove_container(&self, container_id: &str, force: bool) -> AppResult<()>;
    async fn inspect_container(&self, container_id: &str) -> AppResult<ContainerState>;
}

pub struct DockerDevEnvironmentRuntime {
    docker: Arc<DockerClient>,
}

impl DockerDevEnvironmentRuntime {
    pub fn new(docker: Arc<DockerClient>) -> Self {
        Self { docker }
    }
}

#[async_trait]
impl DevEnvironmentRuntime for DockerDevEnvironmentRuntime {
    async fn create_container(&self, config: ContainerConfig) -> AppResult<String> {
        self.docker.create_container(config).await.map_err(|err| {
            ErrorKind::Internal(anyhow::anyhow!("failed to create dev environment container: {err}")).into()
        })
    }

    async fn start_container(&self, container_id: &str) -> AppResult<()> {
        self.docker.start_container(container_id).await.map_err(|err| {
            ErrorKind::Internal(anyhow::anyhow!("failed to start dev environment container: {err}")).into()
        })
    }

    async fn stop_container(&self, container_id: &str, timeout_secs: i64) -> AppResult<()> {
        self.docker.stop_container(container_id, timeout_secs).await.map_err(|err| {
            ErrorKind::Internal(anyhow::anyhow!("failed to stop dev environment container: {err}")).into()
        })
    }

    async fn remove_container(&self, container_id: &str, force: bool) -> AppResult<()> {
        self.docker.remove_container(container_id, force).await.map_err(|err| {
            ErrorKind::Internal(anyhow::anyhow!("failed to remove dev environment container: {err}")).into()
        })
    }

    async fn inspect_container(&self, container_id: &str) -> AppResult<ContainerState> {
        self.docker.inspect_container(container_id).await.map(|info| info.status).map_err(|err| {
            ErrorKind::Internal(anyhow::anyhow!("failed to inspect dev environment container: {err}")).into()
        })
    }
}

/// Business logic layer for dev environment operations.
pub struct DevEnvironmentService<R = DevEnvironmentRepository> {
    repo: R,
    runtime: Option<Arc<dyn DevEnvironmentRuntime>>,
}

impl DevEnvironmentService<DevEnvironmentRepository> {
    pub fn from_runtime(pool: PgPool, runtime: Option<Arc<dyn DevEnvironmentRuntime>>) -> Self {
        Self::with_runtime(DevEnvironmentRepository::new(pool), runtime)
    }
}

impl<R> DevEnvironmentService<R>
where
    R: DevEnvironmentStore,
{
    pub fn new(repo: R) -> Self {
        Self { repo, runtime: None }
    }

    pub fn with_runtime(repo: R, runtime: Option<Arc<dyn DevEnvironmentRuntime>>) -> Self {
        Self { repo, runtime }
    }

    /// List dev environments for the org.
    pub async fn list(&self, scope: &TenantScope) -> AppResult<Vec<DevEnvironment>> {
        let envs = self.repo.list(scope).await?;
        let mut reconciled = Vec::with_capacity(envs.len());
        for env in envs {
            reconciled.push(self.reconcile_runtime_status(scope, env).await?);
        }
        Ok(reconciled)
    }

    /// Get a single dev environment.
    pub async fn get(&self, scope: &TenantScope, id: Uuid) -> AppResult<DevEnvironment> {
        let env = self.repo.get(scope, id).await?;
        self.reconcile_runtime_status(scope, env).await
    }

    /// Create a new dev environment.
    pub async fn create(
        &self,
        scope: &TenantScope,
        name: &str,
        project_id: Option<Uuid>,
        config: &serde_json::Value,
    ) -> AppResult<DevEnvironment> {
        let name = DevEnvironmentName::parse(name)?;
        self.repo.create(scope, name.value(), project_id, config).await
    }

    /// Start a dev environment by provisioning and starting its container.
    pub async fn start(&self, scope: &TenantScope, id: Uuid) -> AppResult<DevEnvironment> {
        let env = self.repo.get(scope, id).await?;
        DevEnvironmentLifecyclePolicy::ensure_can_start(&env.status, env.container_id.as_deref())?;

        let runtime = self.runtime.as_ref().ok_or_else(docker_unavailable)?;
        let config = build_container_config(scope, &env)?;
        self.repo.update_status(scope, id, STARTING_STATUS, None).await?;

        let container_id = match runtime.create_container(config).await {
            Ok(container_id) => container_id,
            Err(err) => {
                let _ = self.repo.update_status(scope, id, ERROR_STATUS, None).await;
                return Err(err);
            }
        };

        if let Err(err) = runtime.start_container(&container_id).await {
            let remove_result = runtime.remove_container(&container_id, true).await;
            let retained_container_id = remove_result.as_ref().err().map(|_| container_id.as_str());
            let _ = self.repo.update_status(scope, id, ERROR_STATUS, retained_container_id).await;
            return Err(err);
        }

        self.repo.update_status(scope, id, RUNNING_STATUS, Some(&container_id)).await
    }

    /// Stop and remove a dev environment container, then clear its container ID.
    pub async fn stop(&self, scope: &TenantScope, id: Uuid) -> AppResult<DevEnvironment> {
        let env = self.repo.get(scope, id).await?;

        let StopPlan::StopContainer(container_id) =
            DevEnvironmentLifecyclePolicy::stop_plan(&env.status, env.container_id.as_deref())?
        else {
            return self.repo.update_status(scope, id, STOPPED_STATUS, None).await;
        };

        let runtime = self.runtime.as_ref().ok_or_else(docker_unavailable)?;
        if let Err(err) = runtime.stop_container(container_id, DEFAULT_STOP_TIMEOUT_SECONDS).await {
            let _ = self.repo.update_status(scope, id, ERROR_STATUS, Some(container_id)).await;
            return Err(err);
        }
        if let Err(err) = runtime.remove_container(container_id, true).await {
            let _ = self.repo.update_status(scope, id, ERROR_STATUS, Some(container_id)).await;
            return Err(err);
        }

        self.repo.update_status(scope, id, STOPPED_STATUS, None).await
    }

    /// Delete a dev environment.
    pub async fn delete(&self, scope: &TenantScope, id: Uuid) -> AppResult<()> {
        let env = self.repo.get(scope, id).await?;
        DevEnvironmentLifecyclePolicy::ensure_can_delete(&env.status)?;
        self.repo.delete(scope, id).await
    }

    async fn reconcile_runtime_status(&self, scope: &TenantScope, env: DevEnvironment) -> AppResult<DevEnvironment> {
        let Some(container_id) = env.container_id.as_deref() else {
            return Ok(env);
        };
        let Some(runtime) = self.runtime.as_ref() else {
            return Ok(env);
        };

        let state = match runtime.inspect_container(container_id).await {
            Ok(state) => state,
            Err(err) => {
                tracing::warn!(
                    error = ?err,
                    dev_environment_id = %env.id,
                    container_id,
                    "failed to reconcile dev environment container status"
                );
                return Ok(env);
            }
        };

        let runtime_state = runtime_state_from_container(state);
        match DevEnvironmentLifecyclePolicy::reconcile_runtime_status(&env.status, runtime_state) {
            Some(DevEnvironmentStatusUpdate::Running) => {
                self.repo.update_status(scope, env.id.as_uuid(), RUNNING_STATUS, Some(container_id)).await
            }
            Some(DevEnvironmentStatusUpdate::Stopped) => {
                self.repo.update_status(scope, env.id.as_uuid(), STOPPED_STATUS, None).await
            }
            None => Ok(env),
        }
    }
}

fn docker_unavailable() -> AppError {
    ErrorKind::Internal(anyhow::anyhow!("Docker runtime not available for dev environments")).into()
}

fn build_container_config(scope: &TenantScope, env: &DevEnvironment) -> AppResult<ContainerConfig> {
    let spec = DevEnvironmentRuntimeSpec::parse(&env.config)?;
    let mut container_env = spec.env;
    container_env.push(format!("AGENTFORGE_DEV_ENVIRONMENT_ID={}", env.id));
    container_env.push(format!("AGENTFORGE_ORG_ID={}", scope.org_id()));

    let mounts = spec
        .mounts
        .into_iter()
        .map(|mount| Mount { source: mount.source, target: mount.target, read_only: mount.read_only })
        .collect();

    Ok(ContainerConfig {
        image: spec.image,
        name: Some(format!("agentforge-devenv-{}", env.id)),
        working_dir: None,
        env: container_env,
        labels: HashMap::from([
            ("agentforge.dev_environment_id".to_string(), env.id.to_string()),
            ("agentforge.org_id".to_string(), scope.org_id().to_string()),
            ("agentforge.created_by".to_string(), env.created_by.to_string()),
        ]),
        resources: resource_limits_from_spec(spec.resources),
        network: spec.network,
        mounts,
        privileged: false,
        host_pid: false,
        tty: false,
        open_stdin: false,
        attach_stdin: false,
        attach_stdout: false,
        attach_stderr: false,
    })
}

fn runtime_state_from_container(state: ContainerState) -> DevEnvironmentRuntimeState {
    match state {
        ContainerState::Running => DevEnvironmentRuntimeState::Running,
        ContainerState::Stopped => DevEnvironmentRuntimeState::Stopped,
        ContainerState::Dead => DevEnvironmentRuntimeState::Dead,
        ContainerState::Created | ContainerState::Paused | ContainerState::Unknown => DevEnvironmentRuntimeState::Other,
    }
}

fn resource_limits_from_spec(spec: crate::domain::dev_environment::DevEnvironmentResourceSpec) -> ResourceLimits {
    let defaults = ResourceLimits::default();
    ResourceLimits {
        cpu_quota: spec.cpu_quota.or(defaults.cpu_quota),
        memory_bytes: spec.memory_bytes.or(defaults.memory_bytes),
        memory_swap_bytes: spec.memory_swap_bytes.or(defaults.memory_swap_bytes),
        pids_limit: spec.pids_limit.or(defaults.pids_limit),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    use std::sync::Mutex;

    use agentforge_core::{DevEnvironmentId, OrgId, ProjectId, UserId};
    use chrono::Utc;
    use serde_json::json;

    #[derive(Clone)]
    struct MockStore {
        state: Arc<Mutex<MockStoreState>>,
    }

    struct MockStoreState {
        env: DevEnvironment,
        updates: Vec<(String, Option<String>)>,
    }

    impl MockStore {
        fn new(env: DevEnvironment) -> Self {
            Self { state: Arc::new(Mutex::new(MockStoreState { env, updates: Vec::new() })) }
        }

        fn env(&self) -> DevEnvironment {
            self.state.lock().expect("mock store lock").env.clone()
        }

        fn updates(&self) -> Vec<(String, Option<String>)> {
            self.state.lock().expect("mock store lock").updates.clone()
        }
    }

    #[async_trait]
    impl DevEnvironmentStore for MockStore {
        async fn list(&self, _scope: &TenantScope) -> AppResult<Vec<DevEnvironment>> {
            Ok(vec![self.env()])
        }

        async fn get(&self, _scope: &TenantScope, _id: Uuid) -> AppResult<DevEnvironment> {
            Ok(self.env())
        }

        async fn create(
            &self,
            _scope: &TenantScope,
            name: &str,
            project_id: Option<Uuid>,
            config: &serde_json::Value,
        ) -> AppResult<DevEnvironment> {
            let mut state = self.state.lock().expect("mock store lock");
            state.env.name = name.to_string();
            state.env.project_id = project_id.map(ProjectId::from);
            state.env.config = config.clone();
            Ok(state.env.clone())
        }

        async fn update_status(
            &self,
            _scope: &TenantScope,
            _id: Uuid,
            status: &str,
            container_id: Option<&str>,
        ) -> AppResult<DevEnvironment> {
            let mut state = self.state.lock().expect("mock store lock");
            state.env.status = status.to_string();
            state.env.container_id = container_id.map(str::to_string);
            let stored_container_id = state.env.container_id.clone();
            state.updates.push((status.to_string(), stored_container_id));
            Ok(state.env.clone())
        }

        async fn delete(&self, _scope: &TenantScope, _id: Uuid) -> AppResult<()> {
            Ok(())
        }
    }

    #[derive(Default)]
    struct MockRuntime {
        created: Mutex<Vec<ContainerConfig>>,
        starts: Mutex<Vec<String>>,
        stops: Mutex<Vec<(String, i64)>>,
        removes: Mutex<Vec<(String, bool)>>,
        inspects: Mutex<Vec<String>>,
        create_results: Mutex<VecDeque<AppResult<String>>>,
        inspect_results: Mutex<VecDeque<AppResult<ContainerState>>>,
        fail_start: bool,
    }

    impl MockRuntime {
        fn with_container(container_id: &str) -> Self {
            let runtime = Self::default();
            runtime.create_results.lock().expect("create results lock").push_back(Ok(container_id.to_string()));
            runtime
        }

        fn with_inspect_state(state: ContainerState) -> Self {
            let runtime = Self::default();
            runtime.inspect_results.lock().expect("inspect results lock").push_back(Ok(state));
            runtime
        }

        fn created_configs(&self) -> Vec<ContainerConfig> {
            self.created.lock().expect("created lock").clone()
        }

        fn starts(&self) -> Vec<String> {
            self.starts.lock().expect("starts lock").clone()
        }

        fn stops(&self) -> Vec<(String, i64)> {
            self.stops.lock().expect("stops lock").clone()
        }

        fn removes(&self) -> Vec<(String, bool)> {
            self.removes.lock().expect("removes lock").clone()
        }

        fn inspects(&self) -> Vec<String> {
            self.inspects.lock().expect("inspects lock").clone()
        }
    }

    #[async_trait]
    impl DevEnvironmentRuntime for MockRuntime {
        async fn create_container(&self, config: ContainerConfig) -> AppResult<String> {
            self.created.lock().expect("created lock").push(config);
            self.create_results
                .lock()
                .expect("create results lock")
                .pop_front()
                .unwrap_or_else(|| Ok("ctr-default".to_string()))
        }

        async fn start_container(&self, container_id: &str) -> AppResult<()> {
            self.starts.lock().expect("starts lock").push(container_id.to_string());
            if self.fail_start {
                return Err(ErrorKind::Internal(anyhow::anyhow!("start failed")).into());
            }
            Ok(())
        }

        async fn stop_container(&self, container_id: &str, timeout_secs: i64) -> AppResult<()> {
            self.stops.lock().expect("stops lock").push((container_id.to_string(), timeout_secs));
            Ok(())
        }

        async fn remove_container(&self, container_id: &str, force: bool) -> AppResult<()> {
            self.removes.lock().expect("removes lock").push((container_id.to_string(), force));
            Ok(())
        }

        async fn inspect_container(&self, container_id: &str) -> AppResult<ContainerState> {
            self.inspects.lock().expect("inspects lock").push(container_id.to_string());
            self.inspect_results
                .lock()
                .expect("inspect results lock")
                .pop_front()
                .unwrap_or(Ok(ContainerState::Running))
        }
    }

    fn test_scope() -> TenantScope {
        crate::test_support::tenant_scope()
    }

    fn test_env(status: &str, config: serde_json::Value, container_id: Option<&str>) -> DevEnvironment {
        DevEnvironment {
            id: DevEnvironmentId::new(),
            organization_id: OrgId::new(),
            project_id: None,
            name: "dev-env".to_string(),
            config,
            status: status.to_string(),
            container_id: container_id.map(str::to_string),
            created_by: UserId::new(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn build_container_config_accepts_structured_env_mounts_and_resources() {
        let scope = test_scope();
        let env = test_env(
            "stopped",
            json!({
                "image": "ubuntu:22.04",
                "env": {"A": "one", "B": "two"},
                "mounts": [{"source": "/tmp/work", "target": "/workspace", "read_only": true}],
                "network": "agentforge-dev",
                "resources": {"memory_bytes": 268435456}
            }),
            None,
        );

        let config = build_container_config(&scope, &env).expect("container config");
        let expected_name = format!("agentforge-devenv-{}", env.id);

        assert_eq!(config.image, "ubuntu:22.04");
        assert_eq!(config.name.as_deref(), Some(expected_name.as_str()));
        assert!(config.env.contains(&"A=one".to_string()));
        assert!(config.env.contains(&format!("AGENTFORGE_DEV_ENVIRONMENT_ID={}", env.id)));
        assert_eq!(config.network.as_deref(), Some("agentforge-dev"));
        assert_eq!(config.mounts[0].target, "/workspace");
        assert_eq!(config.resources.memory_bytes, Some(268435456));
        assert_eq!(config.resources.cpu_quota, ResourceLimits::default().cpu_quota);
        assert_eq!(config.labels["agentforge.dev_environment_id"], env.id.to_string());
    }

    #[test]
    fn build_container_config_requires_image() {
        let scope = test_scope();
        let env = test_env("stopped", json!({"env": ["A=one"]}), None);

        let err = build_container_config(&scope, &env).expect_err("missing image should fail");

        match err.kind {
            ErrorKind::Validation(message) => assert!(message.contains("config.image is required")),
            other => panic!("expected validation error, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn start_creates_and_starts_container_then_persists_running_status() {
        let scope = test_scope();
        let env = test_env("stopped", json!({"image": "ubuntu:22.04", "env": ["MODE=dev"]}), None);
        let id = env.id.as_uuid();
        let store = MockStore::new(env);
        let runtime = Arc::new(MockRuntime::with_container("ctr-dev"));
        let service = DevEnvironmentService::with_runtime(store.clone(), Some(runtime.clone()));

        let started = service.start(&scope, id).await.expect("start environment");

        assert_eq!(started.status, "running");
        assert_eq!(started.container_id.as_deref(), Some("ctr-dev"));
        assert_eq!(
            store.updates(),
            vec![("starting".to_string(), None), ("running".to_string(), Some("ctr-dev".to_string()))]
        );
        assert_eq!(runtime.starts(), vec!["ctr-dev".to_string()]);
        assert_eq!(runtime.created_configs()[0].image, "ubuntu:22.04");
    }

    #[tokio::test]
    async fn start_marks_error_and_removes_created_container_when_start_fails() {
        let scope = test_scope();
        let env = test_env("stopped", json!({"image": "ubuntu:22.04"}), None);
        let id = env.id.as_uuid();
        let store = MockStore::new(env);
        let mut runtime_value = MockRuntime::with_container("ctr-dev");
        runtime_value.fail_start = true;
        let runtime = Arc::new(runtime_value);
        let service = DevEnvironmentService::with_runtime(store.clone(), Some(runtime.clone()));

        let err = service.start(&scope, id).await.expect_err("start should fail");

        match err.kind {
            ErrorKind::Internal(message) => assert!(message.to_string().contains("start failed")),
            other => panic!("expected internal error, got {other:?}"),
        }
        assert_eq!(store.env().status, "error");
        assert!(store.env().container_id.is_none());
        assert_eq!(runtime.removes(), vec![("ctr-dev".to_string(), true)]);
    }

    #[tokio::test]
    async fn start_rejects_existing_container_reference_to_prevent_leaks() {
        let scope = test_scope();
        let env = test_env("error", json!({"image": "ubuntu:22.04"}), Some("ctr-old"));
        let id = env.id.as_uuid();
        let store = MockStore::new(env);
        let runtime = Arc::new(MockRuntime::with_container("ctr-new"));
        let service = DevEnvironmentService::with_runtime(store.clone(), Some(runtime.clone()));

        let err = service.start(&scope, id).await.expect_err("start should reject existing container");

        match err.kind {
            ErrorKind::Validation(message) => assert!(message.contains("stop it before starting")),
            other => panic!("expected validation error, got {other:?}"),
        }
        assert!(store.updates().is_empty());
        assert!(runtime.created_configs().is_empty());
    }

    #[tokio::test]
    async fn stop_tears_down_container_and_clears_container_id() {
        let scope = test_scope();
        let env = test_env("running", json!({"image": "ubuntu:22.04"}), Some("ctr-dev"));
        let id = env.id.as_uuid();
        let store = MockStore::new(env);
        let runtime = Arc::new(MockRuntime::default());
        let service = DevEnvironmentService::with_runtime(store.clone(), Some(runtime.clone()));

        let stopped = service.stop(&scope, id).await.expect("stop environment");

        assert_eq!(stopped.status, "stopped");
        assert!(stopped.container_id.is_none());
        assert_eq!(runtime.stops(), vec![("ctr-dev".to_string(), DEFAULT_STOP_TIMEOUT_SECONDS)]);
        assert_eq!(runtime.removes(), vec![("ctr-dev".to_string(), true)]);
        assert_eq!(store.updates(), vec![("stopped".to_string(), None)]);
    }

    #[tokio::test]
    async fn get_reconciles_dead_container_to_stopped_and_clears_container_id() {
        let scope = test_scope();
        let env = test_env("running", json!({"image": "ubuntu:22.04"}), Some("ctr-dev"));
        let id = env.id.as_uuid();
        let store = MockStore::new(env);
        let runtime = Arc::new(MockRuntime::with_inspect_state(ContainerState::Dead));
        let service = DevEnvironmentService::with_runtime(store.clone(), Some(runtime.clone()));

        let reconciled = service.get(&scope, id).await.expect("get environment");

        assert_eq!(reconciled.status, "stopped");
        assert!(reconciled.container_id.is_none());
        assert_eq!(runtime.inspects(), vec!["ctr-dev".to_string()]);
        assert_eq!(store.updates(), vec![("stopped".to_string(), None)]);
    }
}

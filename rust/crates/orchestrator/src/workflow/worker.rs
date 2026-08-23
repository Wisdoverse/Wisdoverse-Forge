use std::future::Future;
use std::sync::Arc;
use std::thread;

use anyhow::{Context, anyhow};
use temporalio_sdk::{Worker, WorkerOptions};
use temporalio_sdk_core::{CoreRuntime, RuntimeOptions};
use tokio::sync::oneshot;

use crate::config::Config;
use crate::mcp::client::{OutboundMcp, OutboundMcpClient};
use crate::realtime::Broadcaster;

use super::activities::WorkflowActivities;
use super::runtime::WorkflowRuntime;
use super::service::WorkflowService;
use super::store::Store;
use super::temporal::{OrchestratorWorkflow, TASK_QUEUE, TemporalWorkflowRuntime, connect_temporal_client};

/// Honest state of the Temporal-backed workflow runtime at boot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkflowRuntimeStatus {
    /// Connected and the worker is running.
    Up,
    /// Intentionally off (`temporal_enabled=false`) or no store configured.
    Disabled,
    /// Enabled but Temporal could not be reached at boot.
    Unreachable,
}

impl WorkflowRuntimeStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            WorkflowRuntimeStatus::Up => "up",
            WorkflowRuntimeStatus::Disabled => "disabled",
            WorkflowRuntimeStatus::Unreachable => "unreachable",
        }
    }
}

pub struct WorkflowRuntimeComponents {
    pub service: Arc<WorkflowService>,
    pub worker: WorkflowWorkerHandle,
}

pub struct WorkflowWorkerHandle {
    shutdown: oneshot::Sender<()>,
    join: thread::JoinHandle<anyhow::Result<()>>,
}

impl WorkflowWorkerHandle {
    pub async fn shutdown(self) -> anyhow::Result<()> {
        let _ = self.shutdown.send(());
        self.join.join().map_err(|_| anyhow!("workflow worker thread panicked"))?
    }
}

pub async fn build_live_workflow_components(
    config: &Config,
    store: Option<Arc<dyn Store>>,
    outbound_mcp: Option<Arc<dyn OutboundMcp>>,
    broadcaster: Option<Arc<Broadcaster>>,
) -> anyhow::Result<Option<WorkflowRuntimeComponents>> {
    build_live_workflow_components_with_factory(
        config,
        store,
        outbound_mcp,
        broadcaster,
        |config: Config| async move { connect_temporal_client(&config).await },
        |client, outbound_mcp, store, broadcaster| {
            let activities = WorkflowActivities::new(outbound_mcp, store, broadcaster);
            start_worker(client, activities)
        },
    )
    .await
}

pub async fn build_live_workflow_components_with_factory<Connect, ConnectFut, Start>(
    config: &Config,
    store: Option<Arc<dyn Store>>,
    outbound_mcp: Option<Arc<dyn OutboundMcp>>,
    broadcaster: Option<Arc<Broadcaster>>,
    connect: Connect,
    start: Start,
) -> anyhow::Result<Option<WorkflowRuntimeComponents>>
where
    Connect: Fn(Config) -> ConnectFut,
    ConnectFut: Future<Output = anyhow::Result<temporalio_client::Client>>,
    Start: Fn(
        temporalio_client::Client,
        Arc<dyn OutboundMcp>,
        Arc<dyn Store>,
        Option<Arc<Broadcaster>>,
    ) -> anyhow::Result<WorkflowWorkerHandle>,
{
    let Some(store) = store else {
        return Ok(None);
    };
    if !config.temporal_enabled {
        return Ok(None);
    }

    let outbound_mcp = match outbound_mcp {
        Some(outbound_mcp) => outbound_mcp,
        None => Arc::new(OutboundMcpClient::new(config.mcp_endpoint.clone(), config.mcp_token.clone())?)
            as Arc<dyn OutboundMcp>,
    };

    let client = connect(config.clone()).await?;
    let runtime: Arc<dyn WorkflowRuntime> = Arc::new(TemporalWorkflowRuntime::new(client.clone(), store.clone()));
    let service = Arc::new(WorkflowService::new(store.clone(), runtime));
    let worker = start(client, outbound_mcp, store, broadcaster)?;
    Ok(Some(WorkflowRuntimeComponents { service, worker }))
}

/// Build the workflow runtime, classifying failures instead of propagating them.
/// A Temporal outage at boot becomes `Unreachable` (API keeps serving) rather
/// than aborting the process. Applies the configured connect timeout.
pub async fn build_workflow_runtime(
    config: &Config,
    store: Option<Arc<dyn Store>>,
    outbound_mcp: Option<Arc<dyn OutboundMcp>>,
    broadcaster: Option<Arc<Broadcaster>>,
) -> (Option<WorkflowRuntimeComponents>, WorkflowRuntimeStatus) {
    let timeout = std::time::Duration::from_secs(config.temporal_connect_timeout_secs.max(1));
    build_workflow_runtime_with_factory(
        config,
        store,
        outbound_mcp,
        broadcaster,
        move |config: Config| async move {
            match tokio::time::timeout(timeout, connect_temporal_client(&config)).await {
                Ok(result) => result,
                Err(_) => Err(anyhow!("temporal connect timed out after {}s", timeout.as_secs())),
            }
        },
        |client, outbound_mcp, store, broadcaster| {
            let activities = WorkflowActivities::new(outbound_mcp, store, broadcaster);
            start_worker(client, activities)
        },
    )
    .await
}

/// Test seam: same classification, injectable connect/start factories.
pub async fn build_workflow_runtime_with_factory<Connect, ConnectFut, Start>(
    config: &Config,
    store: Option<Arc<dyn Store>>,
    outbound_mcp: Option<Arc<dyn OutboundMcp>>,
    broadcaster: Option<Arc<Broadcaster>>,
    connect: Connect,
    start: Start,
) -> (Option<WorkflowRuntimeComponents>, WorkflowRuntimeStatus)
where
    Connect: Fn(Config) -> ConnectFut,
    ConnectFut: Future<Output = anyhow::Result<temporalio_client::Client>>,
    Start: Fn(
        temporalio_client::Client,
        Arc<dyn OutboundMcp>,
        Arc<dyn Store>,
        Option<Arc<Broadcaster>>,
    ) -> anyhow::Result<WorkflowWorkerHandle>,
{
    // Distinguish "intentionally off" from "tried and failed": when temporal is
    // disabled or no store is configured, the inner builder returns Ok(None).
    if !config.temporal_enabled || store.is_none() {
        return (None, WorkflowRuntimeStatus::Disabled);
    }
    match build_live_workflow_components_with_factory(config, store, outbound_mcp, broadcaster, connect, start).await {
        Ok(Some(components)) => (Some(components), WorkflowRuntimeStatus::Up),
        Ok(None) => (None, WorkflowRuntimeStatus::Disabled),
        Err(err) => {
            tracing::error!(
                error = %err,
                temporal_host = %config.temporal_host,
                temporal_namespace = %config.temporal_namespace,
                "Temporal unreachable at boot; orchestrator serving in degraded (API-only) mode"
            );
            (None, WorkflowRuntimeStatus::Unreachable)
        }
    }
}

pub fn start_worker(
    client: temporalio_client::Client,
    activities: WorkflowActivities,
) -> anyhow::Result<WorkflowWorkerHandle> {
    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
    let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel::<anyhow::Result<()>>(1);

    let join = thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .context("build workflow worker runtime")?;

        runtime.block_on(async move {
            let core_runtime = CoreRuntime::new_assume_tokio(RuntimeOptions::default())?;
            let worker_options = WorkerOptions::new(TASK_QUEUE.to_string())
                .register_activities(activities)
                .register_workflow::<OrchestratorWorkflow>()
                .context("register orchestrator workflow")?
                .build();
            let mut worker = Worker::new(&core_runtime, client, worker_options)
                .map_err(|err| anyhow!(err.to_string()))
                .context("create workflow worker")?;

            ready_tx.send(Ok(())).ok();
            let shutdown = worker.shutdown_handle();
            let run = worker.run();
            tokio::pin!(run);

            tokio::select! {
                result = &mut run => result.context("run workflow worker"),
                _ = shutdown_rx => {
                    shutdown();
                    run.await.context("run workflow worker after shutdown")
                }
            }
        })
    });

    match ready_rx.recv() {
        Ok(Ok(())) => Ok(WorkflowWorkerHandle { shutdown: shutdown_tx, join }),
        Ok(Err(err)) => {
            let _ = join.join();
            Err(err)
        }
        Err(_) => {
            let _ = join.join();
            Err(anyhow!("workflow worker failed before signaling readiness"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::repository::MemoryStore;
    use super::*;

    #[test]
    fn status_as_str_maps_each_variant() {
        assert_eq!(WorkflowRuntimeStatus::Up.as_str(), "up");
        assert_eq!(WorkflowRuntimeStatus::Disabled.as_str(), "disabled");
        assert_eq!(WorkflowRuntimeStatus::Unreachable.as_str(), "unreachable");
    }

    #[tokio::test]
    async fn build_runtime_disabled_when_temporal_off() {
        let config = Config { temporal_enabled: false, ..Default::default() };
        let store: Option<Arc<dyn Store>> = Some(Arc::new(MemoryStore::default()));
        let (components, status) = build_workflow_runtime(&config, store, None, None).await;
        assert!(components.is_none());
        assert_eq!(status, WorkflowRuntimeStatus::Disabled);
    }

    #[tokio::test]
    async fn build_runtime_unreachable_when_connect_fails() {
        let config = Config { temporal_enabled: true, ..Default::default() };
        let store: Option<Arc<dyn Store>> = Some(Arc::new(MemoryStore::default()));
        let (components, status) = build_workflow_runtime_with_factory(
            &config,
            store,
            None,
            None,
            |_c| async { Err(anyhow::anyhow!("temporal down")) },
            |_c, _m, _s, _b| Err(anyhow::anyhow!("unreachable")),
        )
        .await;
        assert!(components.is_none());
        assert_eq!(status, WorkflowRuntimeStatus::Unreachable);
    }
}

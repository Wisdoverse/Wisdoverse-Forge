use std::future::Future;
use std::sync::Arc;
use std::thread;

use anyhow::{Context, anyhow};
use temporalio_sdk::{Worker, WorkerOptions};
use temporalio_sdk_core::{CoreRuntime, RuntimeOptions};
use tokio::sync::oneshot;

use crate::config::Config;
use crate::mcp::client::{OutboundMcp, OutboundMcpClient};

use super::activities::WorkflowActivities;
use super::runtime::WorkflowRuntime;
use super::service::WorkflowService;
use super::store::Store;
use super::temporal::{OrchestratorWorkflow, TASK_QUEUE, TemporalWorkflowRuntime, connect_temporal_client};

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
) -> anyhow::Result<Option<WorkflowRuntimeComponents>> {
    build_live_workflow_components_with_factory(
        config,
        store,
        outbound_mcp,
        |config: Config| async move { connect_temporal_client(&config).await },
        |client, outbound_mcp, store| {
            let activities = WorkflowActivities::new(outbound_mcp, store);
            start_worker(client, activities)
        },
    )
    .await
}

pub async fn build_live_workflow_components_with_factory<Connect, ConnectFut, Start>(
    config: &Config,
    store: Option<Arc<dyn Store>>,
    outbound_mcp: Option<Arc<dyn OutboundMcp>>,
    connect: Connect,
    start: Start,
) -> anyhow::Result<Option<WorkflowRuntimeComponents>>
where
    Connect: Fn(Config) -> ConnectFut,
    ConnectFut: Future<Output = anyhow::Result<temporalio_client::Client>>,
    Start: Fn(temporalio_client::Client, Arc<dyn OutboundMcp>, Arc<dyn Store>) -> anyhow::Result<WorkflowWorkerHandle>,
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
    let worker = start(client, outbound_mcp, store)?;
    Ok(Some(WorkflowRuntimeComponents { service, worker }))
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

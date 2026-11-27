use agentforge_orchestrator::{AppState, Config};
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = Config::load()?;

    // CN-4: optional OTLP span export (no-op when OTEL_EXPORTER_OTLP_ENDPOINT is
    // unset). `_otel_guard` must live for the whole process so the batch exporter
    // flushes on exit; it is dropped when `main` returns.
    let (otel_layer, _otel_guard) = match agentforge_telemetry::otel_layer::<tracing_subscriber::Registry>(
        "agentforge-orchestrator",
        env!("CARGO_PKG_VERSION"),
    ) {
        Some((layer, guard)) => (Some(layer), Some(guard)),
        None => (None, None),
    };
    tracing_subscriber::registry()
        .with(otel_layer)
        .with(EnvFilter::new(&config.log_level))
        .with(tracing_subscriber::fmt::layer().json())
        .init();

    let (state, workflow_worker) = AppState::live_with_runtime(config).await?;

    if let Some(pool) = state.pool.clone() {
        let ttl = state.config.dispatch_timeout_secs;
        let leader_election = state.config.leader_election_enabled;
        tokio::spawn(async move {
            agentforge_orchestrator::dispatch_reaper::DispatchReaperWorker::new(pool, ttl, leader_election).run().await;
            tracing::error!(
                "dispatch reaper loop exited unexpectedly — stuck task_dispatches will no longer be aged out"
            );
        });
    } else {
        tracing::warn!(
            "dispatch reaper not started: orchestrator has no database pool — stuck task_dispatches will not be aged out"
        );
    }

    if state.config.review_escalation_enabled {
        if let Some(pool) = state.pool.clone() {
            let broadcaster = state.broadcaster.clone();
            let audit_store = state.audit_store.clone();
            let grace = state.config.review_escalation_grace_secs;
            let leader_election = state.config.leader_election_enabled;
            tokio::spawn(async move {
                agentforge_orchestrator::review_escalation_reaper::ReviewEscalationReaperWorker::new(
                    pool,
                    broadcaster,
                    audit_store,
                    grace,
                    leader_election,
                )
                .run()
                .await;
                tracing::error!(
                    "review escalation reaper loop exited unexpectedly — overdue reviews will no longer be escalated"
                );
            });
        } else {
            tracing::warn!(
                "review escalation reaper enabled but orchestrator has no database pool — overdue reviews will not be escalated"
            );
        }
    }

    // #1352: end `running` rows whose Temporal execution is closed or proven
    // missing. Needs the database and the Temporal configuration; it connects
    // to Temporal lazily, so it also recovers when Temporal starts later.
    if state.config.workflow_reconcile_enabled && state.config.temporal_enabled {
        match (state.workflow_store.clone(), state.pool.clone()) {
            (Some(store), Some(pool)) => {
                let config = state.config.clone();
                let leader_pool = config.leader_election_enabled.then_some(pool);
                let connect =
                    agentforge_orchestrator::workflow_reconciler::temporal_connector(config.clone(), store.clone());
                tokio::spawn(async move {
                    agentforge_orchestrator::workflow_reconciler::WorkflowReconcilerWorker::new(
                        store,
                        connect,
                        leader_pool,
                        config.temporal_namespace.clone(),
                        config.workflow_reconcile_grace_secs,
                        config.workflow_reconcile_trust_current_temporal,
                    )
                    .run()
                    .await;
                    tracing::error!(
                        "workflow reconciler loop exited unexpectedly — running workflows without a Temporal execution will no longer be reconciled"
                    );
                });
            }
            _ => tracing::warn!("workflow reconciler not started: it needs the orchestrator database"),
        }
    }

    let app = state.clone().router();

    let addr = format!("{}:{}", state.config.host, state.config.port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!(%addr, "orchestrator listening");
    axum::serve(listener, app).with_graceful_shutdown(shutdown_signal()).await?;

    if let Some(workflow_worker) = workflow_worker {
        workflow_worker.shutdown().await?;
    }
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };

    #[cfg(unix)]
    let terminate = async {
        if let Ok(mut signal) = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            let _ = signal.recv().await;
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {}
        _ = terminate => {}
    }
}

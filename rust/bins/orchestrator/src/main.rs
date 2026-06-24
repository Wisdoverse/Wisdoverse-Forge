use agentforge_orchestrator::{AppState, Config};
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = Config::load()?;

    tracing_subscriber::registry()
        .with(EnvFilter::new(&config.log_level))
        .with(tracing_subscriber::fmt::layer().json())
        .init();

    let (state, workflow_worker) = AppState::live_with_runtime(config).await?;

    if let Some(pool) = state.pool.clone() {
        let ttl = state.config.dispatch_timeout_secs;
        tokio::spawn(async move {
            agentforge_orchestrator::dispatch_reaper::DispatchReaperWorker::new(pool, ttl).run().await;
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
            tokio::spawn(async move {
                agentforge_orchestrator::review_escalation_reaper::ReviewEscalationReaperWorker::new(
                    pool,
                    broadcaster,
                    audit_store,
                    grace,
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

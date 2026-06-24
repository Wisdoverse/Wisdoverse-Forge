use std::process::ExitCode;

#[tokio::main]
async fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();

    // Install SIGINT + SIGTERM handlers that cancel the tokio runtime.
    // Matches `cli/main.go` which wires both `os.Interrupt` and `syscall.SIGTERM`.
    // On non-unix we fall back to ctrl_c() only.
    let code = tokio::select! {
        code = agentforge_cli::run(args) => code,
        _ = wait_for_shutdown_signal() => 130,
    };
    ExitCode::from(code.clamp(0, 255) as u8)
}

#[cfg(unix)]
async fn wait_for_shutdown_signal() {
    use tokio::signal::unix::{SignalKind, signal};
    let mut sigterm = match signal(SignalKind::terminate()) {
        Ok(s) => s,
        Err(_) => {
            let _ = tokio::signal::ctrl_c().await;
            return;
        }
    };
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {},
        _ = sigterm.recv() => {},
    }
}

#[cfg(not(unix))]
async fn wait_for_shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
}

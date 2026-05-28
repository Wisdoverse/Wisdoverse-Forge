//! Wisdoverse Forge CLI — Rust port of the Go `cli/` tree.

pub mod auth;
pub mod build_info;
pub mod client;
pub mod cmd;
pub mod config;
pub mod context;
pub mod error;
pub mod global_flags;
pub mod interactive;
pub mod otel;
pub mod output;

use std::io::Write;

pub async fn run(args: Vec<String>) -> i32 {
    let info = build_info::BuildInfo::from_env();
    let cli = match cmd::root::parse_args(info.clone(), args) {
        Ok(cli) => cli,
        Err(e) => {
            let _ = e.print();
            return if e.use_stderr() { 2 } else { 0 };
        }
    };

    let mut stderr = std::io::stderr();
    let mut stdout = std::io::stdout();
    let mut stdin = std::io::BufReader::new(std::io::stdin());

    // Save flags before cli.command is consumed by the dispatch match.
    let flags = cli.flags.clone();

    let (ctx, shutdown) = match cmd::prelude::build_context(&flags, &info, &mut stderr) {
        Ok(v) => v,
        Err(e) => {
            let _ = writeln!(stderr, "Error: {e}");
            return 1;
        }
    };

    let command_path = cli.command_path();
    let span_guard = if std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT").is_ok() || flags.trace {
        Some(otel::start_command(&command_path, &info.version))
    } else {
        None
    };

    let result: error::CliResult<()> = match cli.command {
        Some(cmd::root::Subcommand::Auth(a)) => {
            cmd::auth::dispatch(a, &flags, &ctx, &mut stdin, &mut stdout, &mut stderr)
        }
        Some(cmd::root::Subcommand::Config(a)) => cmd::config::dispatch(a, &ctx, &mut stdout),
        Some(cmd::root::Subcommand::Agents(a)) => {
            cmd::agents::dispatch(a, &ctx, &mut stdin, &mut stdout, &mut stderr).await
        }
        Some(cmd::root::Subcommand::Events(a)) => cmd::events::dispatch(a, &ctx, &mut stdout, &mut stderr).await,
        Some(cmd::root::Subcommand::Groups(a)) => {
            cmd::groups::dispatch(a, &ctx, &mut stdin, &mut stdout, &mut stderr).await
        }
        Some(cmd::root::Subcommand::Api(a)) => cmd::api::run(a, &ctx, &mut stdout, &mut stderr).await,
        Some(cmd::root::Subcommand::Migrate(a)) => cmd::migrate::dispatch(a).await,
        Some(cmd::root::Subcommand::Verify(a)) => cmd::verify::run(a, &mut stdout),
        Some(cmd::root::Subcommand::Health) => cmd::health::run(&ctx, &mut stdout).await,
        Some(cmd::root::Subcommand::Whoami) => cmd::whoami::run(&ctx, &mut stdout).await,
        Some(cmd::root::Subcommand::Version) => cmd::version::run(&info, &ctx, &mut stdout, &mut stderr).await,
        Some(cmd::root::Subcommand::Completion(a)) => cmd::completion::run(a, &mut stdout),
        None => Ok(()),
    };

    // End the command span BEFORE shutting down the tracer so the root span
    // is actually exported. Matches the Go `PersistentPostRunE` order of
    // `endSpan(); tracingShutdown();` in cli/cmd/root.go.
    drop(span_guard);
    if let Some(f) = shutdown {
        f();
    }

    match result {
        Ok(()) => 0,
        Err(e) => {
            write_error(&mut stderr, &ctx.format, &e);
            e.exit_code()
        }
    }
}

fn write_error(stderr: &mut dyn Write, format: &str, err: &error::CliError) {
    use error::CliError;
    if format == "json" {
        // Bind the formatted fallback message to a local so we don't hand
        // `output::json::write_error` a `&str` borrowed from a temporary.
        let fallback;
        let (code, message) = match err {
            CliError::Api(api) => (api.code.as_str(), api.message.as_str()),
            _ => {
                fallback = err.to_string();
                ("ERROR", fallback.as_str())
            }
        };
        let _ = output::json::write_error(stderr, code, message);
        return;
    }
    if let CliError::Api(api) = err {
        let _ = writeln!(stderr, "Error: {} ({})", api.message, api.code);
        return;
    }
    let _ = writeln!(stderr, "Error: {err}");
}

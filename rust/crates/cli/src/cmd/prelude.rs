use crate::build_info::BuildInfo;
use crate::client::{Client, ClientOptions};
use crate::config::Config;
use crate::context::CliContext;
use crate::global_flags::GlobalFlags;
use std::sync::Arc;
use std::time::Duration;

/// Callback returned by [`build_context`] to shut down the OpenTelemetry
/// tracer provider. The caller must invoke it before process exit.
pub type ShutdownFn = Box<dyn FnOnce() + 'static>;

/// Resolves context from global flags + config + env, matching
/// `cli/cmd/root.go:PersistentPreRunE`.
pub fn build_context(
    flags: &GlobalFlags,
    info: &BuildInfo,
    stderr: &mut dyn std::io::Write,
) -> anyhow::Result<(CliContext, Option<ShutdownFn>)> {
    crate::interactive::setup(flags.non_interactive, flags.verbose);

    if flags.token.is_some() {
        let _ = writeln!(
            stderr,
            "Warning: --token is visible in process listings. Prefer AGENTFORGE_TOKEN env var or 'af auth login --token -'."
        );
    }
    if flags.insecure {
        let _ = writeln!(stderr, "WARNING: TLS certificate verification disabled. Connection is NOT secure.");
    }

    let cfg = crate::config::load(&crate::config::default_path())?;

    // Server resolution: flag > config > env (via cfg.resolve_server) > localhost default.
    let server_from_flag = flags.server.clone().filter(|s| !s.is_empty());
    let resolved = server_from_flag.unwrap_or_else(|| cfg.resolve_server());
    let server_trimmed = resolved.trim().to_string();
    let server = if server_trimmed.is_empty() { "http://localhost:4003".to_string() } else { server_trimmed };

    let (token, _src) = crate::auth::resolve(flags.token.as_deref(), &crate::auth::default_credentials_path());

    let timeout = parse_duration_or_err(&flags.timeout)?;

    let format = resolve_output_format(flags, &cfg);

    let trace_enabled = flags.trace || std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT").is_ok();
    // Clone before the closure boundary so the returned Box<dyn FnOnce()>
    // doesn't borrow from `info` (which would prevent `'static`).
    let svc_name = info.name.clone();
    let svc_version = info.version.clone();
    let shutdown: Option<ShutdownFn> = if trace_enabled {
        match crate::otel::init(&svc_name, &svc_version) {
            Ok(f) => Some(Box::new(f)),
            Err(e) => {
                let _ = writeln!(stderr, "Warning: failed to initialize tracing: {e}");
                None
            }
        }
    } else {
        None
    };

    let client = Client::new(ClientOptions {
        server,
        token,
        timeout,
        insecure: flags.insecure,
        verbose: flags.verbose,
        debug: flags.debug,
        trace: trace_enabled,
    })?;

    let ctx = CliContext {
        client: Arc::new(client),
        format,
        jq: flags.jq.clone().unwrap_or_default(),
        cancel: tokio_util::sync::CancellationToken::new(),
    };
    Ok((ctx, shutdown))
}

fn parse_duration_or_err(s: &str) -> anyhow::Result<Duration> {
    // Accept Go-style: 30s, 2m, 1h, 500ms, 1h30m.
    humantime::parse_duration(s).map_err(|e| anyhow::anyhow!("parse timeout {s:?}: {e}"))
}

/// Matches `cli/cmd/root.go:resolveOutputFormat`.
pub fn resolve_output_format(flags: &GlobalFlags, cfg: &Config) -> String {
    if flags.quiet {
        return "quiet".into();
    }
    if flags.json {
        return "json".into();
    }
    if let Some(o) = &flags.output
        && !o.is_empty()
    {
        return o.clone();
    }
    if let Ok(v) = std::env::var("AGENTFORGE_OUTPUT")
        && !v.is_empty()
    {
        return v;
    }
    if !cfg.defaults.output.is_empty() {
        return cfg.defaults.output.clone();
    }
    if !crate::interactive::is_interactive() {
        return "json".into();
    }
    "table".into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::global_flags::GlobalFlags;

    #[test]
    fn format_resolution_priority() {
        let mut cfg = crate::config::Config::default();
        cfg.defaults.output = "table".into();

        let f = GlobalFlags { timeout: "30s".into(), quiet: true, ..GlobalFlags::default() };
        assert_eq!(resolve_output_format(&f, &cfg), "quiet");

        let f2 = GlobalFlags { timeout: "30s".into(), json: true, ..GlobalFlags::default() };
        assert_eq!(resolve_output_format(&f2, &cfg), "json");

        let f3 = GlobalFlags { timeout: "30s".into(), output: Some("yaml".into()), ..GlobalFlags::default() };
        assert_eq!(resolve_output_format(&f3, &cfg), "yaml");
    }
}

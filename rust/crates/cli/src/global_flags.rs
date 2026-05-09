use clap::Args;

/// Global flags applied before any subcommand runs.
/// Field order and names must match `cli/cmd/root.go:GlobalFlags` + the
/// PersistentFlags block in `newRootCmd`.
#[derive(Debug, Clone, Default, Args)]
pub struct GlobalFlags {
    /// Auth token (overrides stored credentials)
    #[arg(long, global = true)]
    pub token: Option<String>,

    /// Server URL (overrides config)
    #[arg(long, global = true)]
    pub server: Option<String>,

    /// Organization ID (overrides config)
    #[arg(long, global = true)]
    pub org: Option<String>,

    /// Output format: table, json, yaml
    #[arg(short = 'o', long, global = true)]
    pub output: Option<String>,

    /// JSON output (shorthand for -o json)
    #[arg(long, global = true)]
    pub json: bool,

    /// Output IDs only
    #[arg(short = 'q', long, global = true)]
    pub quiet: bool,

    /// jq expression to filter/transform output
    #[arg(long, global = true)]
    pub jq: Option<String>,

    /// Skip TLS certificate verification
    #[arg(long, global = true)]
    pub insecure: bool,

    /// Request timeout (e.g. 30s, 2m)
    #[arg(long, global = true, default_value = "30s")]
    pub timeout: String,

    /// Show HTTP method + URL + status on stderr
    #[arg(short = 'v', long, global = true)]
    pub verbose: bool,

    /// Show full request/response on stderr
    #[arg(long, global = true)]
    pub debug: bool,

    /// Disable prompts and progress output
    #[arg(long, global = true)]
    pub non_interactive: bool,

    /// Enable OpenTelemetry tracing
    #[arg(long, global = true)]
    pub trace: bool,
}

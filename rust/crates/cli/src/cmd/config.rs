use crate::config;
use crate::context::CliContext;
use crate::error::{CliError, CliResult};
use crate::output;
use clap::Subcommand;
use serde_json::json;
use std::io::Write;

#[derive(Debug, clap::Args)]
#[command(subcommand_required = true, arg_required_else_help = true)]
pub struct ConfigArgs {
    #[command(subcommand)]
    pub command: ConfigSubcommand,
}

#[derive(Debug, Subcommand)]
pub enum ConfigSubcommand {
    /// Set a config value
    Set { key: String, value: String },
    /// Get a config value
    Get { key: String },
    /// List all config values
    List,
}

pub fn dispatch(args: ConfigArgs, ctx: &CliContext, stdout: &mut dyn Write) -> CliResult<()> {
    match args.command {
        ConfigSubcommand::Set { key, value } => set(key, value, ctx, stdout),
        ConfigSubcommand::Get { key } => get(key, ctx, stdout),
        ConfigSubcommand::List => list(ctx, stdout),
    }
}

fn set(key: String, value: String, ctx: &CliContext, stdout: &mut dyn Write) -> CliResult<()> {
    let path = config::default_path();
    let mut cfg = config::load(&path).map_err(|e| CliError::Other(format!("load config: {e}")))?;
    cfg.set(&key, &value).map_err(|e| CliError::Other(e.to_string()))?;
    config::save(&path, &cfg).map_err(|e| CliError::Other(format!("save config: {e}")))?;
    output::format_action(
        stdout,
        &ctx.format,
        &format!("{key} = {value}"),
        &json!({ "key": key, "value": value, "updated": true }),
    )
    .map_err(|e| CliError::Other(e.to_string()))?;
    Ok(())
}

fn get(key: String, ctx: &CliContext, stdout: &mut dyn Write) -> CliResult<()> {
    let cfg = config::load(&config::default_path()).map_err(|e| CliError::Other(format!("load config: {e}")))?;
    let val = cfg.get(&key).map_err(|e| CliError::Other(e.to_string()))?;
    if matches!(ctx.format.as_str(), "json" | "yaml" | "quiet") {
        output::format(stdout, &ctx.format, &[], &json!({ "key": key, "value": val }), None)
            .map_err(|e| CliError::Other(e.to_string()))?;
    } else {
        writeln!(stdout, "{val}").ok();
    }
    Ok(())
}

fn list(ctx: &CliContext, stdout: &mut dyn Write) -> CliResult<()> {
    let cfg = config::load(&config::default_path()).map_err(|e| CliError::Other(format!("load config: {e}")))?;
    let m = cfg.list();
    if m.is_empty() {
        if matches!(ctx.format.as_str(), "json" | "yaml" | "quiet") {
            output::format(stdout, &ctx.format, &[], &json!({}), None).map_err(|e| CliError::Other(e.to_string()))?;
        } else {
            writeln!(stdout, "(no config values set)").ok();
        }
        return Ok(());
    }
    if matches!(ctx.format.as_str(), "json" | "yaml" | "quiet") {
        let v = serde_json::to_value(&m).map_err(|e| CliError::Other(e.to_string()))?;
        output::format(stdout, &ctx.format, &[], &v, None).map_err(|e| CliError::Other(e.to_string()))?;
    } else {
        for (k, v) in m {
            writeln!(stdout, "{k} = {v}").ok();
        }
    }
    Ok(())
}

use std::path::PathBuf;

use agent_context_helper::SUPPORTED_CONTEXT_ENVELOPE_VERSIONS;
use agent_context_helper::cli_adapter::claude::apply_claude_adapter;
use agent_context_helper::cli_adapter::codex::apply_codex_adapter;
use agent_context_helper::cli_adapter::gemini::apply_gemini_adapter;
use agent_context_helper::cli_adapter::opencode::apply_opencode_adapter;
use agentforge_core::context_envelope::ContextEnvelope;
use anyhow::{Context, Result, anyhow};

#[derive(Debug, Default)]
struct Args {
    print_supported_versions: bool,
    adapter: Option<String>,
    envelope: Option<PathBuf>,
    home: Option<PathBuf>,
    report: Option<PathBuf>,
}

fn main() -> Result<()> {
    let args = parse_args()?;
    if args.print_supported_versions {
        println!("{}", SUPPORTED_CONTEXT_ENVELOPE_VERSIONS.join("\n"));
        return Ok(());
    }

    let adapter = args.adapter.as_deref().ok_or_else(|| anyhow!("--adapter is required"))?;
    let envelope_path = args.envelope.as_ref().ok_or_else(|| anyhow!("--envelope is required"))?;
    let home = args.home.unwrap_or_else(default_home);
    let envelope: ContextEnvelope = serde_json::from_slice(
        &std::fs::read(envelope_path).with_context(|| format!("read {}", envelope_path.display()))?,
    )
    .with_context(|| format!("decode {}", envelope_path.display()))?;

    let report = match adapter {
        "claude" => apply_claude_adapter(&envelope, &home)?,
        "codex" => apply_codex_adapter(&envelope, &home)?,
        "gemini" => apply_gemini_adapter(&envelope, &home)?,
        "opencode" => apply_opencode_adapter(&envelope, &home)?,
        other => return Err(anyhow!("unsupported adapter: {other}")),
    };

    if let Some(path) = args.report {
        let bytes = serde_json::to_vec_pretty(&report).context("encode adapter report")?;
        std::fs::write(&path, bytes).with_context(|| format!("write {}", path.display()))?;
    }
    Ok(())
}

fn parse_args() -> Result<Args> {
    let mut args = Args::default();
    let mut iter = std::env::args().skip(1);
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--print-supported-versions" => args.print_supported_versions = true,
            "--adapter" => args.adapter = Some(next_value(&mut iter, "--adapter")?),
            "--envelope" => args.envelope = Some(PathBuf::from(next_value(&mut iter, "--envelope")?)),
            "--home" => args.home = Some(PathBuf::from(next_value(&mut iter, "--home")?)),
            "--report" => args.report = Some(PathBuf::from(next_value(&mut iter, "--report")?)),
            "-h" | "--help" => {
                print_help();
                std::process::exit(0);
            }
            other => return Err(anyhow!("unknown argument: {other}")),
        }
    }
    Ok(args)
}

fn next_value(iter: &mut impl Iterator<Item = String>, flag: &str) -> Result<String> {
    iter.next().ok_or_else(|| anyhow!("{flag} requires a value"))
}

fn default_home() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/home/agent"))
}

fn print_help() {
    println!("agent-context-helper --print-supported-versions");
    println!(
        "agent-context-helper --adapter <claude|codex|gemini|opencode> --envelope <path> [--home <path>] [--report <path>]"
    );
}

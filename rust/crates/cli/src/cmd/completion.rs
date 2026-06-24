use crate::error::{CliError, CliResult};
use clap::CommandFactory;
use clap_complete::{Shell, generate};
use std::io::Write;
use std::str::FromStr;

#[derive(Debug, Clone, clap::Args)]
pub struct CompletionArgs {
    /// Target shell (bash, zsh, fish, powershell)
    #[arg(value_parser = ["bash", "zsh", "fish", "powershell"])]
    pub shell: String,
}

pub fn run(args: CompletionArgs, stdout: &mut dyn Write) -> CliResult<()> {
    let shell = Shell::from_str(&args.shell)
        .map_err(|e| CliError::Other(format!("unsupported shell {:?}: {e}", args.shell)))?;
    let mut cmd = crate::cmd::root::Cli::command();
    let bin_name = cmd.get_name().to_string();
    generate(shell, &mut cmd, bin_name, stdout);
    Ok(())
}

use crate::context::CliContext;
use crate::error::CliResult;
use crate::output::Column;
use clap::Subcommand;
use std::io::{BufRead, Write};

pub mod collaborators;
pub mod create;
pub mod delete;
pub mod enroll_local;
pub mod get;
pub mod interrupt;
pub mod keys;
pub mod list;
pub mod move_;
pub mod permission;
pub mod prompt;
pub mod restart;
pub mod transfer;
pub mod update;
pub mod wait;

/// Default agent columns for table output.
pub const COLUMNS: &[Column] = &[
    Column { header: "ID", field: "id" },
    Column { header: "NAME", field: "name" },
    Column { header: "TOOL", field: "cliTool" },
    Column { header: "STATUS", field: "status" },
    Column { header: "PROJECT", field: "projectId" },
    Column { header: "CREATED", field: "createdAt" },
];

#[derive(Debug, clap::Args)]
#[command(subcommand_required = true, arg_required_else_help = true)]
pub struct AgentsArgs {
    #[command(subcommand)]
    pub command: AgentsSubcommand,
}

#[derive(Debug, Subcommand)]
pub enum AgentsSubcommand {
    /// List agents
    List(list::ListArgs),
    /// Get an agent by ID
    Get(get::GetArgs),
    /// Create a new agent
    Create(create::CreateArgs),
    /// Connect a local work tool as a managed Agent
    #[command(
        long_about = "Connects a local work tool to the remote Forge platform so it can receive tasks as a managed Agent.\n\nBefore you run this command, connect to your Forge server with `agentforge config set server https://forge.example.com` and sign in with `agentforge auth login --token <platform-token>`.\n\nRun it from the local folder where the Agent is allowed to work. Success looks like a launch block for Terminal or PowerShell, then the Agent appears online in the web UI after the sidecar starts."
    )]
    EnrollLocal(enroll_local::EnrollLocalArgs),
    /// Update an agent
    Update(update::UpdateArgs),
    /// Delete an agent
    Delete(delete::DeleteArgs),
    /// Move an agent to a different project
    Move(move_::MoveArgs),
    /// Send a prompt to an agent (or broadcast to a group)
    Prompt(prompt::PromptArgs),
    /// Interrupt a running agent
    Interrupt(interrupt::InterruptArgs),
    /// Restart an agent's container
    Restart(restart::RestartArgs),
    /// Push environment variable keys to an agent
    Keys(keys::KeysArgs),
    /// Respond to an agent permission request
    Permission(permission::PermissionArgs),
    /// Transfer agent ownership
    Transfer(transfer::TransferArgs),
    /// Wait for an agent to reach a target status or event
    Wait(wait::WaitArgs),
    /// Manage agent collaborators
    Collaborators(collaborators::CollaboratorsArgs),
}

pub async fn dispatch(
    args: AgentsArgs,
    ctx: &CliContext,
    stdin: &mut dyn BufRead,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> CliResult<()> {
    match args.command {
        AgentsSubcommand::List(a) => list::run(a, ctx, stdout).await,
        AgentsSubcommand::Get(a) => get::run(a, ctx, stdout).await,
        AgentsSubcommand::Create(a) => create::run(a, ctx, stdout, stderr).await,
        AgentsSubcommand::EnrollLocal(a) => enroll_local::run(a, ctx, stdout).await,
        AgentsSubcommand::Update(a) => update::run(a, ctx, stdout).await,
        AgentsSubcommand::Delete(a) => delete::run(a, ctx, stdin, stdout, stderr).await,
        AgentsSubcommand::Move(a) => move_::run(a, ctx, stdout).await,
        AgentsSubcommand::Prompt(a) => prompt::run(a, ctx, stdin, stdout, stderr).await,
        AgentsSubcommand::Interrupt(a) => interrupt::run(a, ctx, stdout).await,
        AgentsSubcommand::Restart(a) => restart::run(a, ctx, stdout).await,
        AgentsSubcommand::Keys(a) => keys::run(a, ctx, stdout).await,
        AgentsSubcommand::Permission(a) => permission::run(a, ctx, stdout).await,
        AgentsSubcommand::Transfer(a) => transfer::run(a, ctx, stdout).await,
        AgentsSubcommand::Wait(a) => wait::run(a, ctx, stdout).await,
        AgentsSubcommand::Collaborators(a) => collaborators::run(a, ctx, stdout).await,
    }
}

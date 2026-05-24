use crate::build_info::BuildInfo;
use crate::global_flags::GlobalFlags;
use clap::{CommandFactory, FromArgMatches, Parser};

#[derive(Parser)]
#[command(
    name = "agentforge",
    about = "Wisdoverse Forge CLI — manage AI agents from the command line",
    long_about = "Wisdoverse Forge CLI provides shell-based access to the Wisdoverse Forge platform for developers and AI agents.",
    disable_help_subcommand = true,
    arg_required_else_help = true
)]
pub struct Cli {
    #[command(flatten)]
    pub flags: GlobalFlags,

    #[command(subcommand)]
    pub command: Option<Subcommand>,
}

#[derive(clap::Subcommand)]
pub enum Subcommand {
    Auth(crate::cmd::auth::AuthArgs),
    Config(crate::cmd::config::ConfigArgs),
    Agents(crate::cmd::agents::AgentsArgs),
    Events(crate::cmd::events::EventsArgs),
    Groups(crate::cmd::groups::GroupsArgs),
    Api(crate::cmd::api::ApiArgs),
    Health,
    Whoami,
    Version,
    Completion(crate::cmd::completion::CompletionArgs),
}

pub fn parse_args(info: BuildInfo, args: Vec<String>) -> Result<Cli, clap::Error> {
    // Leak the version/name strings so they satisfy clap's `'static` requirement.
    let version: &'static str = Box::leak(info.version.into_boxed_str());
    let name: &'static str = Box::leak(info.name.into_boxed_str());
    let cmd = Cli::command().version(version).name(name);
    let matches = cmd.try_get_matches_from(args)?;
    Cli::from_arg_matches(&matches)
}

impl Cli {
    /// Returns a command path suitable for tracing span naming (e.g. "agents/list", "auth", "health").
    /// Matches the Go CLI's `cmd.CommandPath()` which produces `agentforge agents list`, minus the root name.
    pub fn command_path(&self) -> String {
        match &self.command {
            None => String::new(),
            Some(Subcommand::Auth(a)) => {
                use crate::cmd::auth::AuthSubcommand;
                match &a.command {
                    AuthSubcommand::Login { .. } => "auth/login".into(),
                    AuthSubcommand::Logout => "auth/logout".into(),
                    AuthSubcommand::Status => "auth/status".into(),
                }
            }
            Some(Subcommand::Config(a)) => {
                use crate::cmd::config::ConfigSubcommand;
                match &a.command {
                    ConfigSubcommand::Set { .. } => "config/set".into(),
                    ConfigSubcommand::Get { .. } => "config/get".into(),
                    ConfigSubcommand::List => "config/list".into(),
                }
            }
            Some(Subcommand::Agents(a)) => {
                use crate::cmd::agents::AgentsSubcommand;
                match &a.command {
                    AgentsSubcommand::List(_) => "agents/list".into(),
                    AgentsSubcommand::Get(_) => "agents/get".into(),
                    AgentsSubcommand::Create(_) => "agents/create".into(),
                    AgentsSubcommand::EnrollLocal(_) => "agents/enroll-local".into(),
                    AgentsSubcommand::Update(_) => "agents/update".into(),
                    AgentsSubcommand::Delete(_) => "agents/delete".into(),
                    AgentsSubcommand::Move(_) => "agents/move".into(),
                    AgentsSubcommand::Prompt(_) => "agents/prompt".into(),
                    AgentsSubcommand::Interrupt(_) => "agents/interrupt".into(),
                    AgentsSubcommand::Restart(_) => "agents/restart".into(),
                    AgentsSubcommand::Keys(_) => "agents/keys".into(),
                    AgentsSubcommand::Permission(_) => "agents/permission".into(),
                    AgentsSubcommand::Transfer(_) => "agents/transfer".into(),
                    AgentsSubcommand::Wait(_) => "agents/wait".into(),
                    AgentsSubcommand::Collaborators(c) => {
                        use crate::cmd::agents::collaborators::Sub;
                        match &c.command {
                            Sub::List { .. } => "agents/collaborators/list".into(),
                            Sub::Add { .. } => "agents/collaborators/add".into(),
                            Sub::Remove { .. } => "agents/collaborators/remove".into(),
                        }
                    }
                }
            }
            Some(Subcommand::Events(a)) => {
                use crate::cmd::events::EventsSubcommand;
                match &a.command {
                    EventsSubcommand::List(_) => "events/list".into(),
                    EventsSubcommand::Watch(_) => "events/watch".into(),
                    EventsSubcommand::Stats(_) => "events/stats".into(),
                }
            }
            Some(Subcommand::Groups(a)) => {
                use crate::cmd::groups::{GroupsSubcommand, WorkersSubcommand};
                match &a.command {
                    GroupsSubcommand::List(_) => "groups/list".into(),
                    GroupsSubcommand::Create(_) => "groups/create".into(),
                    GroupsSubcommand::Get(_) => "groups/get".into(),
                    GroupsSubcommand::Update(_) => "groups/update".into(),
                    GroupsSubcommand::Delete(_) => "groups/delete".into(),
                    GroupsSubcommand::Workers(w) => match &w.command {
                        WorkersSubcommand::List(_) => "groups/workers/list".into(),
                        WorkersSubcommand::Add(_) => "groups/workers/add".into(),
                        WorkersSubcommand::Remove(_) => "groups/workers/remove".into(),
                    },
                    GroupsSubcommand::Dispatch(_) => "groups/dispatch".into(),
                }
            }
            Some(Subcommand::Api(_)) => "api".into(),
            Some(Subcommand::Health) => "health".into(),
            Some(Subcommand::Whoami) => "whoami".into(),
            Some(Subcommand::Version) => "version".into(),
            Some(Subcommand::Completion(_)) => "completion".into(),
        }
    }
}

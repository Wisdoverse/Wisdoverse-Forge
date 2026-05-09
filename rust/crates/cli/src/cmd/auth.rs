use crate::auth;
use crate::context::CliContext;
use crate::error::CliResult;
use crate::global_flags::GlobalFlags;
use crate::output;
use clap::Subcommand;
use serde_json::json;
use std::io::{BufRead, Write};

#[derive(Debug, clap::Args)]
#[command(subcommand_required = true, arg_required_else_help = true)]
pub struct AuthArgs {
    #[command(subcommand)]
    pub command: AuthSubcommand,
}

#[derive(Debug, Subcommand)]
pub enum AuthSubcommand {
    /// Store an auth token
    Login {
        /// API token to store (use `-` to read from stdin)
        #[arg(long)]
        token: String,
    },
    /// Remove stored auth token
    Logout,
    /// Show current auth status
    Status,
}

pub fn dispatch(
    args: AuthArgs,
    flags: &GlobalFlags,
    ctx: &CliContext,
    stdin: &mut dyn BufRead,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> CliResult<()> {
    match args.command {
        AuthSubcommand::Login { token } => login(token, ctx, stdin, stdout, stderr),
        AuthSubcommand::Logout => logout(ctx, stdout),
        AuthSubcommand::Status => status(flags, ctx, stdout),
    }
}

fn login(
    token_arg: String,
    ctx: &CliContext,
    stdin: &mut dyn BufRead,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> CliResult<()> {
    let token = if token_arg == "-" {
        let mut line = String::new();
        stdin.read_line(&mut line).map_err(|e| crate::error::CliError::Other(format!("read token from stdin: {e}")))?;
        line.trim().to_string()
    } else {
        token_arg
    };
    if token.is_empty() {
        return Err(crate::error::CliError::Other(
            "token must not be empty; use --token <value> or --token - to read from stdin".into(),
        ));
    }
    let path = auth::default_credentials_path();
    auth::store(&path, &token).map_err(|e| crate::error::CliError::Other(e.to_string()))?;
    writeln!(stderr, "Note: token is stored in plaintext. Keep this file private (permissions: 0600).").ok();
    output::format_action(
        stdout,
        &ctx.format,
        &format!("Token stored at {}", path.display()),
        &json!({ "path": path.display().to_string(), "stored": true }),
    )
    .map_err(|e| crate::error::CliError::Other(e.to_string()))?;
    Ok(())
}

fn logout(ctx: &CliContext, stdout: &mut dyn Write) -> CliResult<()> {
    let path = auth::default_credentials_path();
    auth::delete(&path).map_err(|e| crate::error::CliError::Other(e.to_string()))?;
    output::format_action(
        stdout,
        &ctx.format,
        &format!("Credentials removed from {}", path.display()),
        &json!({ "path": path.display().to_string(), "removed": true }),
    )
    .map_err(|e| crate::error::CliError::Other(e.to_string()))?;
    Ok(())
}

fn status(flags: &GlobalFlags, ctx: &CliContext, stdout: &mut dyn Write) -> CliResult<()> {
    let path = auth::default_credentials_path();
    let (tok, src) = auth::resolve(flags.token.as_deref(), &path);

    match tok {
        None => {
            if matches!(ctx.format.as_str(), "json" | "yaml" | "quiet") {
                output::format(stdout, &ctx.format, &[], &json!({ "authenticated": false }), None)
                    .map_err(|e| crate::error::CliError::Other(e.to_string()))?;
            } else {
                writeln!(stdout, "Not authenticated. Run 'auth login --token <token>' to store credentials.").ok();
            }
        }
        Some(t) => {
            let prefix = if t.len() > 8 { format!("{}...", &t[..8]) } else { t.clone() };
            let result = json!({
                "authenticated": true, "source": src.as_str(), "token": prefix
            });
            if matches!(ctx.format.as_str(), "json" | "yaml" | "quiet") {
                output::format(stdout, &ctx.format, &[], &result, None)
                    .map_err(|e| crate::error::CliError::Other(e.to_string()))?;
            } else {
                writeln!(stdout, "Authenticated").ok();
                writeln!(stdout, "  Source : {}", src.as_str()).ok();
                writeln!(stdout, "  Token  : {}", result["token"].as_str().unwrap_or("")).ok();
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    use tempfile::tempdir;

    #[test]
    fn login_writes_token_file() {
        let d = tempdir().unwrap();
        let xdg = d.path().to_owned();
        temp_env::with_var("XDG_CONFIG_HOME", Some(xdg.as_os_str()), || {
            let mut stdin = Cursor::new(Vec::new());
            let mut stdout = Vec::new();
            let mut stderr = Vec::new();
            let ctx = CliContext {
                client: std::sync::Arc::new(
                    crate::client::Client::new(crate::client::ClientOptions {
                        server: "http://localhost:4003".into(),
                        token: None,
                        timeout: std::time::Duration::from_secs(30),
                        insecure: false,
                        verbose: false,
                        debug: false,
                        trace: false,
                    })
                    .unwrap(),
                ),
                format: "json".into(),
                jq: String::new(),
                cancel: tokio_util::sync::CancellationToken::new(),
            };
            login("abc".into(), &ctx, &mut stdin, &mut stdout, &mut stderr).unwrap();
            let stored = std::fs::read_to_string(crate::auth::default_credentials_path()).unwrap();
            assert_eq!(stored.trim(), "abc");
        });
    }
}

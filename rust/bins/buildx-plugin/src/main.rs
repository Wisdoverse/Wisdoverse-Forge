use std::io::{self, Write};
use std::process::ExitCode;

use agentforge_buildx_plugin::{normalize_args, plugin_metadata, run_build};

#[tokio::main]
async fn main() -> ExitCode {
    let raw_args: Vec<String> = std::env::args().skip(1).collect();
    let args = normalize_args(&raw_args);

    if matches!(args.first(), Some(command) if command == "docker-cli-plugin-metadata") {
        let mut stdout = io::stdout();
        if serde_json::to_writer(&mut stdout, &plugin_metadata()).is_ok() && writeln!(stdout).is_ok() {
            return ExitCode::SUCCESS;
        }
        let _ = writeln!(io::stderr(), "build error: failed to write plugin metadata");
        return ExitCode::from(1);
    }

    if !matches!(args.first(), Some(command) if command == "build") {
        let _ = writeln!(io::stderr(), "usage: docker buildx build [OPTIONS] PATH");
        return ExitCode::from(1);
    }

    let mut stdout = io::stdout();
    match run_build(&args[1..], &mut stdout).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            let _ = writeln!(io::stderr(), "build error: {err}");
            ExitCode::from(1)
        }
    }
}

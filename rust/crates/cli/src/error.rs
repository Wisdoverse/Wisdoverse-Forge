pub use crate::client::api_error::ApiError;
use thiserror::Error;

/// Top-level error type for CLI commands.
/// Exit codes match the Go CLI's behavior in `cli/cmd/root.go:exitCodeFromError`.
#[derive(Debug, Error)]
pub enum CliError {
    #[error(transparent)]
    Api(#[from] ApiError),

    /// Wait/stream deadline exceeded. Exit 6.
    #[error("{0}")]
    WaitTimeout(String),

    /// SSE stream error. Exit 7.
    #[error("stream: {0}")]
    Stream(String),

    /// Transport-layer failure (connection reset, DNS, TLS, etc.). Exit 1.
    /// Distinguished from `Other` so `agents wait` / `agents prompt --wait` can
    /// trigger the poll-fallback path. Matches Go's `net.OpError` detection in
    /// `cli/cmd/agents/prompt.go:isPromptTransportError`.
    #[error("{0}")]
    Transport(String),

    /// Confirmation required in non-interactive mode. Exit 2.
    #[error("{0}")]
    ConfirmationRequired(String),

    /// Context canceled (SIGINT/SIGTERM). Exit 130.
    #[error("canceled")]
    Canceled,

    /// Any other error. Exit 1.
    #[error("{0}")]
    Other(String),
}

impl CliError {
    pub fn exit_code(&self) -> i32 {
        match self {
            CliError::Api(e) => e.exit_code(),
            CliError::WaitTimeout(_) => 6,
            CliError::Stream(_) => 7,
            CliError::Transport(_) => 1,
            CliError::ConfirmationRequired(_) => 2,
            CliError::Canceled => 130,
            CliError::Other(_) => 1,
        }
    }

    pub fn other(msg: impl Into<String>) -> Self {
        CliError::Other(msg.into())
    }
}

impl From<anyhow::Error> for CliError {
    fn from(e: anyhow::Error) -> Self {
        if let Some(api) = e.downcast_ref::<ApiError>() {
            return CliError::Api(api.clone());
        }
        CliError::Other(format!("{e:#}"))
    }
}

pub type CliResult<T> = Result<T, CliError>;

/// Exit-code table shown in the root `--help`. Must agree with
/// [`crate::client::api_error::API_ERROR_EXIT_CODES`] and [`CliError::exit_code`]
/// (checked by tests, together with docs/guides/cli-platform-support.md).
pub const EXIT_CODES_HELP: &str = "\
Exit codes:
  0    Success.
  1    Any other failure: API errors with any other code (for example
       provider_rate_limited, provider_credentials_rejected,
       provider_request_rejected, RATE_LIMITED, HTTP_ERROR), transport errors.
  2    VALIDATION_ERROR, a usage error, or a confirmation that needs --force.
  3    UNAUTHORIZED or INVALID_API_KEY.
  4    NOT_FOUND.
  5    FORBIDDEN.
  6    A wait or stream deadline was exceeded.
  7    The event stream failed.
  130  Canceled (Ctrl+C or SIGTERM).
Scripts should treat any non-zero exit code as failure.";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exit_code_for_error_matches_go_parity() {
        assert_eq!(ApiError::exit_code_for("UNAUTHORIZED"), 3);
        assert_eq!(ApiError::exit_code_for("INVALID_API_KEY"), 3);
        assert_eq!(ApiError::exit_code_for("NOT_FOUND"), 4);
        assert_eq!(ApiError::exit_code_for("FORBIDDEN"), 5);
        assert_eq!(ApiError::exit_code_for("VALIDATION_ERROR"), 2);
        assert_eq!(ApiError::exit_code_for("SOMETHING_ELSE"), 1);
    }

    /// `(exit code, meaning)` rows from a Markdown table or from `EXIT_CODES_HELP`.
    fn exit_code_rows(text: &str) -> Vec<(i32, String)> {
        let mut rows: Vec<(i32, String)> = Vec::new();
        for line in text.lines() {
            let cells: Vec<&str> = line.split('|').map(str::trim).collect();
            if cells.len() >= 4
                && let Ok(exit) = cells[1].parse::<i32>()
            {
                rows.push((exit, cells[2].to_string()));
            } else if let Some((exit, meaning)) = line.strip_prefix("  ").and_then(|l| l.split_once(' '))
                && let Ok(exit) = exit.parse::<i32>()
            {
                rows.push((exit, meaning.trim().to_string()));
            } else if line.starts_with("       ")
                && let Some(last) = rows.last_mut()
            {
                last.1.push(' ');
                last.1.push_str(line.trim());
            }
        }
        rows
    }

    #[test]
    fn exit_code_tables_in_help_and_guide_match_the_mapping() {
        use crate::client::api_error::API_ERROR_EXIT_CODES;
        const GUIDE: &str = include_str!("../../../../docs/guides/cli-platform-support.md");

        use clap::CommandFactory;
        let help = crate::cmd::root::Cli::command().render_long_help().to_string();
        assert!(help.contains("Exit codes:"), "root --help must show the exit-code table");

        // Only the guide's own exit-code section (other tables there have numeric first cells too).
        // A Windows checkout can give the guide CRLF line endings.
        let guide = GUIDE.replace("\r\n", "\n");
        let guide_section = guide.split("\n## Exit Codes\n").nth(1).expect("guide has an Exit Codes section");
        let guide_section = guide_section.split("\n## ").next().unwrap_or(guide_section);
        for (source, text) in [("--help", EXIT_CODES_HELP), ("cli-platform-support.md", guide_section)] {
            let rows = exit_code_rows(text);
            let meaning = |exit: i32| -> &str {
                rows.iter()
                    .find(|(e, _)| *e == exit)
                    .map(|(_, m)| m.as_str())
                    .unwrap_or_else(|| panic!("{source}: missing exit code {exit}"))
            };
            // Every mapped API code is listed under its exit code ...
            for (code, exit) in API_ERROR_EXIT_CODES {
                assert!(meaning(*exit).contains(code), "{source}: exit {exit} must list {code}");
            }
            // ... and every code named in a row maps to that row's exit code.
            for (exit, text) in &rows {
                let codes = text.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'));
                for code in codes.filter(|w| w.contains('_')) {
                    assert_eq!(ApiError::exit_code_for(code), *exit, "{source}: {code} listed under exit {exit}");
                }
            }
            // Non-API exits.
            assert!(meaning(1).contains("transport"), "{source}");
            assert_eq!(CliError::Transport(String::new()).exit_code(), 1);
            assert!(meaning(2).contains("--force"), "{source}");
            assert_eq!(CliError::ConfirmationRequired(String::new()).exit_code(), 2);
            assert!(meaning(6).contains("deadline"), "{source}");
            assert_eq!(CliError::WaitTimeout(String::new()).exit_code(), 6);
            assert!(meaning(7).contains("stream"), "{source}");
            assert_eq!(CliError::Stream(String::new()).exit_code(), 7);
            assert!(meaning(130).contains("Canceled"), "{source}");
            assert_eq!(CliError::Canceled.exit_code(), 130);
            assert!(meaning(0).contains("Success"), "{source}");
        }
    }

    #[test]
    fn cli_error_exit_code_falls_through_from_api_error() {
        let e = CliError::Api(ApiError { code: "NOT_FOUND".into(), message: "no".into(), status: 404 });
        assert_eq!(e.exit_code(), 4);
    }

    #[test]
    fn wait_timeout_variant_has_exit_code_6() {
        assert_eq!(CliError::WaitTimeout("x".into()).exit_code(), 6);
    }

    #[test]
    fn stream_variant_has_exit_code_7() {
        assert_eq!(CliError::Stream("y".into()).exit_code(), 7);
    }

    #[test]
    fn canceled_variant_has_exit_code_130() {
        assert_eq!(CliError::Canceled.exit_code(), 130);
    }

    #[test]
    fn confirmation_required_variant_has_exit_code_2() {
        assert_eq!(CliError::ConfirmationRequired("need --force".into()).exit_code(), 2);
    }

    #[test]
    fn transport_variant_has_exit_code_1() {
        assert_eq!(CliError::Transport("dns lookup failed".into()).exit_code(), 1);
    }

    #[test]
    fn other_variant_has_exit_code_1() {
        assert_eq!(CliError::Other("oops".into()).exit_code(), 1);
    }

    #[test]
    fn from_plain_anyhow_error_maps_to_other_exit_1() {
        let e: CliError = anyhow::anyhow!("plain error").into();
        assert_eq!(e.exit_code(), 1);
        assert!(matches!(e, CliError::Other(_)));
    }

    #[test]
    fn from_anyhow_containing_api_error_downcasts_and_preserves_exit_code() {
        let api = ApiError { code: "NOT_FOUND".into(), message: "agent missing".into(), status: 404 };
        let wrapped = anyhow::Error::from(api);
        let e: CliError = wrapped.into();
        assert_eq!(e.exit_code(), 4, "anyhow-wrapped ApiError must downcast back to Api variant");
        match e {
            CliError::Api(a) => {
                assert_eq!(a.code, "NOT_FOUND");
                assert_eq!(a.message, "agent missing");
            }
            other => panic!("expected CliError::Api, got {other:?}"),
        }
    }
}

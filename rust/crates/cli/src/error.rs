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

use crate::error::CliError;
use std::io::{BufRead, Write};

/// Prompts for confirmation, returning (confirmed, err).
/// Returns `ConfirmationRequired` error (exit 2) when non-interactive + !force.
/// Matches `cli/internal/interactive/confirm.go:ConfirmOrForce`.
pub fn confirm_or_force(
    force: bool,
    prompt: &str,
    stderr: &mut dyn Write,
    stdin: &mut dyn BufRead,
) -> Result<bool, CliError> {
    if force {
        return Ok(true);
    }
    if !super::is_interactive() {
        return Err(CliError::ConfirmationRequired(
            "confirmation required: use --force in non-interactive mode".into(),
        ));
    }
    write!(stderr, "{prompt} [y/N]: ").ok();
    stderr.flush().ok();
    let mut line = String::new();
    stdin.read_line(&mut line).map_err(|e| CliError::Other(format!("read confirmation: {e}")))?;
    let ans = line.trim().to_lowercase();
    Ok(ans == "y" || ans == "yes")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn force_short_circuits() {
        let mut err = Vec::new();
        let mut stdin = Cursor::new(b"".to_vec());
        assert!(confirm_or_force(true, "?", &mut err, &mut stdin).unwrap());
    }

    #[test]
    fn non_interactive_without_force_errors_with_exit_code_2() {
        super::super::setup(true, false);
        let mut err = Vec::new();
        let mut stdin = Cursor::new(b"".to_vec());
        let e = confirm_or_force(false, "?", &mut err, &mut stdin).unwrap_err();
        assert_eq!(e.exit_code(), 2);
    }
}

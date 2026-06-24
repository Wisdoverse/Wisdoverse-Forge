pub mod confirm;

use std::sync::atomic::{AtomicBool, Ordering};

static NON_INTERACTIVE: AtomicBool = AtomicBool::new(false);
static VERBOSE: AtomicBool = AtomicBool::new(false);

/// Sets the interactivity state. Mirrors `Setup` in
/// `cli/internal/interactive/interactive.go`.
pub fn setup(non_interactive_flag: bool, verbose_flag: bool) {
    NON_INTERACTIVE.store(non_interactive_flag || detect_non_interactive(), Ordering::SeqCst);
    VERBOSE.store(verbose_flag, Ordering::SeqCst);
}

pub fn is_interactive() -> bool {
    !NON_INTERACTIVE.load(Ordering::SeqCst)
}

pub fn show_progress() -> bool {
    if !NON_INTERACTIVE.load(Ordering::SeqCst) {
        return true;
    }
    VERBOSE.load(Ordering::SeqCst)
}

/// Matches `cli/internal/interactive/interactive.go:detectNonInteractive`.
pub fn detect_non_interactive() -> bool {
    if let Ok(v) = std::env::var("AGENTFORGE_NON_INTERACTIVE")
        && v.eq_ignore_ascii_case("true")
    {
        return true;
    }
    if let Ok(v) = std::env::var("CI")
        && v.eq_ignore_ascii_case("true")
    {
        return true;
    }
    if std::env::var("TERM").as_deref() == Ok("dumb") {
        return true;
    }
    !is_terminal::IsTerminal::is_terminal(&std::io::stdin())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ci_env_forces_non_interactive() {
        temp_env::with_var("CI", Some("true"), || {
            assert!(detect_non_interactive());
        });
    }
}

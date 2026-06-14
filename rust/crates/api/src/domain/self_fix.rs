//! Self-fix domain policy: pure, no I/O. Sensitive-path circuit breaker + review vocab.

use agentforge_core::{AppError, ErrorKind};
use serde::Serialize;
use serde_json::{json, Value};

/// Glob-prefix directories whose ANY descendant is sensitive.
#[allow(dead_code)]
const SENSITIVE_DIR_PREFIXES: &[&str] = &[
    "rust/crates/auth/",
    "rust/crates/db/migrations/",
    ".github/workflows/",
];

/// Basenames sensitive wherever they appear.
#[allow(dead_code)]
const SENSITIVE_BASENAMES: &[&str] = &["middleware.rs", "mcp.rs", "security.rs"];

/// Exact repo-root-relative files (own-code + CODEOWNERS).
#[allow(dead_code)]
const SENSITIVE_EXACT: &[&str] = &[
    ".github/CODEOWNERS",
    "rust/crates/api/src/services/self_fix/mod.rs",
    "rust/crates/api/src/services/self_fix/bridge.rs",
    "rust/crates/api/src/services/self_fix/import.rs",
    "rust/crates/api/src/services/self_fix/merge_executor.rs",
    "rust/crates/api/src/services/github_app/mod.rs",
    "rust/crates/api/src/routes/self_fix.rs",
    "rust/crates/api/src/domain/self_fix.rs",
];

#[allow(dead_code)]
pub(crate) struct SensitivePathPolicy;

impl SensitivePathPolicy {
    /// True if ANY changed path is sensitive. Input paths are repo-root-relative,
    /// forward-slashed (the form `git diff --name-only` / diff-tree emits).
    #[allow(dead_code)]
    pub(crate) fn touches_sensitive(changed_paths: &[String]) -> bool {
        changed_paths.iter().any(|p| Self::is_sensitive(p))
    }

    fn is_sensitive(path: &str) -> bool {
        if SENSITIVE_DIR_PREFIXES.iter().any(|d| path.starts_with(d)) {
            return true;
        }
        if SENSITIVE_EXACT.contains(&path) {
            return true;
        }
        let basename = path.rsplit('/').next().unwrap_or(path);
        SENSITIVE_BASENAMES.contains(&basename)
    }
}

/// Review status vocabulary (mirrors orchestrator ReviewState; driven API-side).
pub(crate) mod review_status {
    #[allow(dead_code)]
    pub(crate) const IN_REVIEW: &str = "in_review";
    #[allow(dead_code)]
    pub(crate) const APPROVED: &str = "approved";
    #[allow(dead_code)]
    pub(crate) const CHANGES_REQUESTED: &str = "changes_requested";
    #[allow(dead_code)]
    pub(crate) const MERGED: &str = "merged";
    /// Routed to CODEOWNERS / manual merge; in-platform Approve disabled.
    #[allow(dead_code)]
    pub(crate) const SENSITIVE_BLOCKED: &str = "sensitive_blocked";
}

#[allow(dead_code)]
pub(crate) fn self_fix_data_response<T: Serialize>(data: T) -> Value {
    json!({ "ok": true, "data": data })
}

#[allow(dead_code)]
pub(crate) struct SelfFixPolicy;

impl SelfFixPolicy {
    #[allow(dead_code)]
    pub(crate) fn sensitive_path_blocked() -> AppError {
        ErrorKind::ForbiddenWithCode {
            code: "errors.self_fix.sensitive_path_blocked",
            message: "This PR touches a security-sensitive path; in-platform merge is disabled. \
                      Route it to a CODEOWNERS review and merge manually."
                .into(),
        }
        .into()
    }

    #[allow(dead_code)]
    pub(crate) fn checks_not_green() -> AppError {
        ErrorKind::ValidationWithCode {
            code: "errors.self_fix.checks_not_green",
            message: "Required CI checks are not all green; cannot merge.".into(),
        }
        .into()
    }

    #[allow(dead_code)]
    pub(crate) fn head_moved() -> AppError {
        ErrorKind::Conflict("the PR head moved since review; re-review required".into()).into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn blocked(paths: &[&str]) -> bool {
        SensitivePathPolicy::touches_sensitive(&paths.iter().map(|s| s.to_string()).collect::<Vec<_>>())
    }

    #[test]
    fn sensitive_paths_trip_the_breaker() {
        assert!(blocked(&["rust/crates/auth/src/jwt.rs"]));
        assert!(blocked(&["rust/crates/db/migrations/099_x.sql"]));
        assert!(blocked(&["rust/crates/api/src/middleware.rs"]));
        assert!(blocked(&["rust/crates/auth/src/middleware.rs"]));
        assert!(blocked(&["rust/crates/api/src/mcp.rs"]));
        assert!(blocked(&["rust/crates/api/src/domain/mcp.rs"]));
        assert!(blocked(&["rust/crates/api/src/repositories/agent/mcp.rs"]));
        assert!(blocked(&["rust/crates/platform/src/security.rs"]));
        assert!(blocked(&[".github/workflows/ci.yml"]));
        assert!(blocked(&[".github/CODEOWNERS"]));
        assert!(blocked(&["rust/crates/api/src/services/self_fix/bridge.rs"]));
        assert!(blocked(&["rust/crates/api/src/services/self_fix/merge_executor.rs"]));
        assert!(blocked(&["rust/crates/api/src/services/github_app/mod.rs"]));
        assert!(blocked(&["rust/crates/api/src/domain/self_fix.rs"]));
    }

    #[test]
    fn benign_paths_do_not_trip() {
        assert!(!blocked(&["src/app/features/board/TaskCard.tsx"]));
        assert!(!blocked(&["rust/crates/api/src/routes/licenses.rs"]));
        assert!(!blocked(&["docs/guides/configuration.md"]));
    }

    #[test]
    fn a_mix_with_one_sensitive_path_trips() {
        assert!(blocked(&["README.md", "rust/crates/auth/src/jwt.rs"]));
    }

    #[test]
    fn bare_prefix_without_rust_is_not_the_matcher_input_form() {
        assert!(!blocked(&["crates/auth/x.rs"]));
    }
}

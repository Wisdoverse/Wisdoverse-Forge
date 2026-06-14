//! Project git-clone domain rules (M1 — pure types + policy, no IO).
//!
//! This module owns the clone-attempt status vocabulary, the per-attempt state
//! machine, the filesystem-safe workspace directory name policy, and clone
//! error classification/redaction. Everything here is a pure function or value
//! object: no database access, no container orchestration, no business
//! coordination. Services (M2+) own that I/O and convert at the boundary.
//!
//! The status enums back the M0 `TEXT` columns (`projects.clone_status` and
//! `project_clone_attempts.status`). The M0 `db` entity keeps those columns as
//! `String`; conversion to/from these typed enums happens at the service
//! boundary in M2, not in the entity row.
//!
//! Every public item here is a forward-facing M1 domain primitive: the state
//! machine is enforced by the worker in M5, the directory policy by the create
//! path in M2, the URL/redaction helpers by M2/M5/M6. Until those milestones
//! land, the only non-test caller is this module's own tests, so the whole unit
//! carries a module-level `dead_code` allowance (it is exercised by the
//! `#[cfg(test)]` suite below). Remove the allowance as M2+ wires each piece in.

#![allow(dead_code)]

use std::{fmt, path::Path, path::PathBuf, str::FromStr};

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Clone attempt status (source-of-truth `project_clone_attempts.status`)
// ---------------------------------------------------------------------------

/// Lifecycle status of a single clone attempt row.
///
/// Backs `project_clone_attempts.status` (a `TEXT` column constrained by the
/// migration's `project_clone_attempts_status_check`). `as_str` values must
/// match that CHECK list exactly; the drift-guard test ties the two together.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CloneAttemptStatus {
    Queued,
    Cloning,
    Ready,
    Failed,
    Cancelled,
}

impl CloneAttemptStatus {
    /// Every variant, for exhaustive iteration in tests and reconciler sweeps.
    pub const ALL: [Self; 5] = [Self::Queued, Self::Cloning, Self::Ready, Self::Failed, Self::Cancelled];

    /// Stable DB/API slug. Must match `project_clone_attempts_status_check`.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Cloning => "cloning",
            Self::Ready => "ready",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }

    /// A terminal status has no outgoing transition. Retry is modeled as a new
    /// attempt row (created `Queued`), never a transition of an existing one.
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Ready | Self::Failed | Self::Cancelled)
    }

    /// Apply the per-attempt state machine (design spec §7) for ONE attempt:
    ///
    /// ```text
    /// Queued  -> Cloning
    /// Queued  -> Cancelled
    /// Cloning -> Ready
    /// Cloning -> Failed
    /// Cloning -> Cancelled
    /// ```
    ///
    /// `Ready`, `Failed`, and `Cancelled` are terminal (no outgoing edge).
    ///
    /// Retry after a failure is a brand-new attempt row created in `Queued`, so
    /// `Failed -> Queued` is intentionally NOT a legal in-attempt transition.
    pub fn try_transition(self, to: CloneAttemptStatus) -> Result<CloneAttemptStatus, IllegalTransition> {
        let legal = matches!(
            (self, to),
            (Self::Queued, Self::Cloning)
                | (Self::Queued, Self::Cancelled)
                | (Self::Cloning, Self::Ready)
                | (Self::Cloning, Self::Failed)
                | (Self::Cloning, Self::Cancelled)
        );
        if legal { Ok(to) } else { Err(IllegalTransition { from: self, to }) }
    }
}

impl fmt::Display for CloneAttemptStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for CloneAttemptStatus {
    type Err = UnknownCloneStatus;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "queued" => Ok(Self::Queued),
            "cloning" => Ok(Self::Cloning),
            "ready" => Ok(Self::Ready),
            "failed" => Ok(Self::Failed),
            "cancelled" => Ok(Self::Cancelled),
            other => Err(UnknownCloneStatus { raw: other.to_string() }),
        }
    }
}

// ---------------------------------------------------------------------------
// Clone status (denormalized `projects.clone_status` summary)
// ---------------------------------------------------------------------------

/// Denormalized clone summary on the project row, for fast list rendering.
///
/// Backs `projects.clone_status` (a `TEXT` column constrained by the
/// migration's `projects_clone_status_check`). It mirrors the latest attempt's
/// status, except a `Cancelled` attempt collapses to `None` (a cancelled
/// attempt's project shows no active clone). The mapping rule lives in
/// [`CloneStatus::from_attempt`]; M5 denormalization uses it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CloneStatus {
    None,
    Queued,
    Cloning,
    Ready,
    Failed,
}

impl CloneStatus {
    /// Every variant, for exhaustive iteration in the drift-guard test.
    pub const ALL: [Self; 5] = [Self::None, Self::Queued, Self::Cloning, Self::Ready, Self::Failed];

    /// Stable DB/API slug. Must match `projects_clone_status_check`.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Queued => "queued",
            Self::Cloning => "cloning",
            Self::Ready => "ready",
            Self::Failed => "failed",
        }
    }

    /// Denormalize the latest attempt's status onto the project summary column.
    ///
    /// `None` (no attempt yet) and `Some(Cancelled)` both collapse to `None`:
    /// a project with no attempt, or whose only/latest attempt was cancelled,
    /// shows no active clone. All other attempt statuses map 1:1.
    pub fn from_attempt(latest: Option<CloneAttemptStatus>) -> CloneStatus {
        match latest {
            None => Self::None,
            Some(CloneAttemptStatus::Cancelled) => Self::None,
            Some(CloneAttemptStatus::Queued) => Self::Queued,
            Some(CloneAttemptStatus::Cloning) => Self::Cloning,
            Some(CloneAttemptStatus::Ready) => Self::Ready,
            Some(CloneAttemptStatus::Failed) => Self::Failed,
        }
    }
}

impl fmt::Display for CloneStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for CloneStatus {
    type Err = UnknownCloneStatus;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "none" => Ok(Self::None),
            "queued" => Ok(Self::Queued),
            "cloning" => Ok(Self::Cloning),
            "ready" => Ok(Self::Ready),
            "failed" => Ok(Self::Failed),
            other => Err(UnknownCloneStatus { raw: other.to_string() }),
        }
    }
}

// ---------------------------------------------------------------------------
// Typed errors
// ---------------------------------------------------------------------------

/// An attempted state-machine transition that the design spec §7 forbids.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("illegal clone attempt transition: {from} -> {to}")]
pub struct IllegalTransition {
    pub from: CloneAttemptStatus,
    pub to: CloneAttemptStatus,
}

/// A status string that does not match any known DB/API slug.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("unknown clone status: {raw}")]
pub struct UnknownCloneStatus {
    pub raw: String,
}

/// A derived workspace directory name that would escape the projects root.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("workspace dir name {name:?} escapes the projects root")]
pub struct PathEscape {
    pub name: String,
}

// ---------------------------------------------------------------------------
// WorkspaceDirName — filesystem-safe project directory name
// ---------------------------------------------------------------------------

/// Maximum length of a derived workspace directory name.
const WORKSPACE_DIR_NAME_MAX: usize = 64;

/// Safe fallback when derivation yields an empty or reserved name.
const WORKSPACE_DIR_NAME_FALLBACK: &str = "project";

/// Names that are unsafe as a directory component even after character
/// filtering: traversal tokens and the git metadata directory.
const RESERVED_DIR_NAMES: &[&str] = &[".", "..", ".git"];

/// A filesystem-safe directory name under a workspace's projects root.
///
/// Always lowercase, always `[a-z0-9-]`, never empty, never a reserved name,
/// length-capped. Derivation is total: any input produces a usable name (or the
/// [`WORKSPACE_DIR_NAME_FALLBACK`] default).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceDirName {
    value: String,
}

impl WorkspaceDirName {
    /// Derive a filesystem-safe directory name from a project name.
    ///
    /// Rules (mirrors `ResourceSlugPolicy::derive`, then hardens for the
    /// filesystem): lowercase; keep `[a-z0-9-]`; replace any other character run
    /// with a single `-`; collapse repeated `-`; trim leading/trailing `-`; cap
    /// at [`WORKSPACE_DIR_NAME_MAX`]; if the result is empty or a reserved name
    /// (`.`, `..`, `.git`), fall back to [`WORKSPACE_DIR_NAME_FALLBACK`]
    /// (`"project"`).
    pub fn derive(name: &str) -> WorkspaceDirName {
        let mut out = String::with_capacity(name.len().min(WORKSPACE_DIR_NAME_MAX));
        let mut prev_dash = true; // suppress a leading dash

        for ch in name.chars() {
            if ch.is_ascii_alphanumeric() {
                out.push(ch.to_ascii_lowercase());
                prev_dash = false;
            } else if !prev_dash {
                out.push('-');
                prev_dash = true;
            }
        }

        // Cap length first, then re-trim a trailing dash the cap may have left.
        if out.len() > WORKSPACE_DIR_NAME_MAX {
            out.truncate(WORKSPACE_DIR_NAME_MAX);
        }
        while out.ends_with('-') {
            out.pop();
        }

        if out.is_empty() || RESERVED_DIR_NAMES.contains(&out.as_str()) {
            out = WORKSPACE_DIR_NAME_FALLBACK.to_string();
        }

        WorkspaceDirName { value: out }
    }

    /// The validated directory name.
    pub fn as_str(&self) -> &str {
        &self.value
    }

    /// Resolve this name to an absolute path under `root`, proving the result
    /// stays within `root` WITHOUT requiring the target to exist.
    ///
    /// Containment is proven structurally, not by `canonicalize`-ing the target
    /// (which would fail or follow symlinks for a non-existent path):
    ///   1. The derived name is already `[a-z0-9-]`, non-empty, non-reserved —
    ///      it contains no `/`, no `\`, and no `..`, so it cannot introduce a
    ///      parent or absolute component.
    ///   2. We still assert defensively that the joined path's components
    ///      contain no `ParentDir` / `RootDir` / `Prefix` token beyond `root`'s
    ///      own prefix, and that the joined path starts with `root`.
    ///
    /// `root` itself is taken as-is (the caller owns its canonicalization); we
    /// only guarantee the single appended component cannot escape it.
    pub fn resolve_under(&self, root: &Path) -> Result<PathBuf, PathEscape> {
        // Defense in depth: the name is already filtered, but assert it.
        if self.value.is_empty()
            || self.value.contains('/')
            || self.value.contains('\\')
            || self.value.contains("..")
            || RESERVED_DIR_NAMES.contains(&self.value.as_str())
        {
            return Err(PathEscape { name: self.value.clone() });
        }

        let joined = root.join(&self.value);

        // The joined path must be exactly `root` plus one normal component:
        // no parent-dir / root-dir token may appear past root's own prefix.
        let escapes = joined
            .strip_prefix(root)
            .map(|tail| {
                tail.components().any(|c| {
                    matches!(
                        c,
                        std::path::Component::ParentDir
                            | std::path::Component::RootDir
                            | std::path::Component::Prefix(_)
                    )
                })
            })
            .unwrap_or(true); // strip_prefix fails => joined is not under root

        if escapes {
            return Err(PathEscape { name: self.value.clone() });
        }

        Ok(joined)
    }
}

impl fmt::Display for WorkspaceDirName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.value)
    }
}

// ---------------------------------------------------------------------------
// Clone error classification + redaction
// ---------------------------------------------------------------------------

/// Maximum stored length of a redacted clone error message.
const ERROR_MESSAGE_MAX: usize = 500;

/// Coarse classification of a clone failure, derived from git's stderr.
///
/// Backs `project_clone_attempts.error_class`. The classifier matches common
/// git stderr signatures; `Internal` is the catch-all for anything unmatched.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CloneErrorClass {
    Auth,
    NotFound,
    Network,
    Timeout,
    TooLarge,
    Internal,
}

impl CloneErrorClass {
    /// Stable DB/API slug.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Auth => "auth",
            Self::NotFound => "not_found",
            Self::Network => "network",
            Self::Timeout => "timeout",
            Self::TooLarge => "too_large",
            Self::Internal => "internal",
        }
    }

    /// Best-effort classification from a (possibly multi-line) git stderr blob.
    ///
    /// Order matters: timeout is checked before generic network, and auth
    /// before not-found, because real git failures often emit overlapping
    /// phrases. Unmatched input is [`CloneErrorClass::Internal`].
    pub fn classify(raw: &str) -> CloneErrorClass {
        let lc = raw.to_ascii_lowercase();
        let has = |needle: &str| lc.contains(needle);

        if has("timed out") || has("timeout") || has("operation timed out") {
            Self::Timeout
        } else if has("authentication failed")
            || has("could not read username")
            || has("could not read password")
            || has("invalid username or password")
            || has("permission denied")
            || has("403")
        {
            Self::Auth
        } else if has("repository not found") || has("not found") || has("does not exist") || has("404") {
            Self::NotFound
        } else if has("could not resolve host")
            || has("failed to connect")
            || has("connection refused")
            || has("network is unreachable")
            || has("ssl")
            || has("tls")
        {
            Self::Network
        } else if has("pack exceeds maximum allowed size")
            || has("disk quota exceeded")
            || has("no space left on device")
            || has("too large")
        {
            Self::TooLarge
        } else {
            Self::Internal
        }
    }
}

impl fmt::Display for CloneErrorClass {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Redact secrets from a raw clone error blob before it is persisted/displayed.
///
/// Strips userinfo credentials from embedded URLs (`https://user:token@host`
/// -> `https://host`), strips standalone token-looking substrings (provider
/// token prefixes, long hex/base64 runs), collapses whitespace, and truncates
/// to [`ERROR_MESSAGE_MAX`]. Raw stderr (with secrets) stays server-side in
/// logs and is never the value passed forward.
pub fn redact(raw: &str) -> String {
    let mut s = redact_url_userinfo(raw);
    s = redact_token_like(&s);
    s = collapse_whitespace(&s);
    truncate_chars(&s, ERROR_MESSAGE_MAX)
}

/// Replace `scheme://userinfo@host` with `scheme://host` for any embedded URL.
fn redact_url_userinfo(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let bytes = raw.as_bytes();
    let mut i = 0;

    while i < raw.len() {
        // Look for "://" starting at i.
        if raw[i..].starts_with("://") {
            // Emit "://".
            out.push_str("://");
            i += 3;

            // The authority runs until the next path/query/fragment/space
            // delimiter or end of string.
            let auth_start = i;
            let mut j = auth_start;
            while j < raw.len() {
                let c = bytes[j];
                if matches!(c, b'/' | b'?' | b'#' | b' ' | b'\t' | b'\n' | b'\r') {
                    break;
                }
                j += 1;
            }
            let authority = &raw[auth_start..j];
            // Strip userinfo: keep only the part after the LAST '@'.
            let host = authority.rsplit('@').next().unwrap_or(authority);
            out.push_str(host);
            i = j;
        } else {
            // Copy this char verbatim (advance by one full UTF-8 char).
            let ch = raw[i..].chars().next().unwrap_or('\u{FFFD}');
            out.push(ch);
            i += ch.len_utf8();
        }
    }

    out
}

/// Redact standalone token-looking substrings. Conservative: only whole
/// whitespace-delimited tokens that look like secrets are replaced, so prose
/// is preserved. Recognizes provider prefixes (`ghp_`, `gho_`, `ghu_`, `ghs_`,
/// `ghr_`, `github_pat_`, `glpat-`) and long hex/base64 runs.
fn redact_token_like(raw: &str) -> String {
    raw.split_inclusive(char::is_whitespace)
        .map(|chunk| {
            // Separate the token from any trailing whitespace we kept.
            let trimmed_len = chunk.trim_end().len();
            let (tok, ws) = chunk.split_at(trimmed_len);
            // Also peel trailing punctuation so "ghp_secret." still redacts.
            let core_len = tok.trim_end_matches(['.', ',', ';', ':', ')', ']', '"', '\'']).len();
            let (core, punct) = tok.split_at(core_len);
            if looks_like_token(core) { format!("[REDACTED]{punct}{ws}") } else { chunk.to_string() }
        })
        .collect()
}

/// Heuristic: does this bare token look like a secret?
fn looks_like_token(tok: &str) -> bool {
    const PREFIXES: &[&str] = &["ghp_", "gho_", "ghu_", "ghs_", "ghr_", "github_pat_", "glpat-"];
    if PREFIXES.iter().any(|p| tok.starts_with(p)) {
        return true;
    }
    // Long hex run (>= 32) — e.g. an OAuth token or SHA-like secret blob.
    if tok.len() >= 32 && tok.chars().all(|c| c.is_ascii_hexdigit()) {
        return true;
    }
    // Long base64url-ish run (>= 40) with mixed case + digits — entropy proxy.
    if tok.len() >= 40
        && tok.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '+' | '/' | '='))
        && tok.chars().any(|c| c.is_ascii_uppercase())
        && tok.chars().any(|c| c.is_ascii_lowercase())
        && tok.chars().any(|c| c.is_ascii_digit())
    {
        return true;
    }
    false
}

/// Collapse any run of whitespace into a single space, trimming the ends.
fn collapse_whitespace(raw: &str) -> String {
    raw.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Truncate to at most `max` characters (not bytes) without splitting a char.
fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    s.chars().take(max).collect()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    // -- Status enums + state machine ---------------------------------------

    #[test]
    fn clone_attempt_status_as_str_matches_slugs() {
        assert_eq!(CloneAttemptStatus::Queued.as_str(), "queued");
        assert_eq!(CloneAttemptStatus::Cloning.as_str(), "cloning");
        assert_eq!(CloneAttemptStatus::Ready.as_str(), "ready");
        assert_eq!(CloneAttemptStatus::Failed.as_str(), "failed");
        assert_eq!(CloneAttemptStatus::Cancelled.as_str(), "cancelled");
    }

    #[test]
    fn clone_attempt_status_roundtrips_str() {
        for status in CloneAttemptStatus::ALL {
            let parsed: CloneAttemptStatus = status.as_str().parse().expect("round-trip parse");
            assert_eq!(parsed, status);
        }
        assert!("bogus".parse::<CloneAttemptStatus>().is_err());
        assert!("".parse::<CloneAttemptStatus>().is_err());
        // Case-sensitive: DB slugs are lowercase only.
        assert!("Queued".parse::<CloneAttemptStatus>().is_err());
    }

    #[test]
    fn clone_attempt_status_serde_uses_snake_case() {
        let json = serde_json::to_string(&CloneAttemptStatus::Cancelled).unwrap();
        assert_eq!(json, "\"cancelled\"");
        let back: CloneAttemptStatus = serde_json::from_str("\"cloning\"").unwrap();
        assert_eq!(back, CloneAttemptStatus::Cloning);
    }

    #[test]
    fn legal_transitions_are_accepted() {
        use CloneAttemptStatus::*;
        let legal = [(Queued, Cloning), (Queued, Cancelled), (Cloning, Ready), (Cloning, Failed), (Cloning, Cancelled)];
        for (from, to) in legal {
            assert_eq!(from.try_transition(to).unwrap(), to, "{from} -> {to} should be legal");
        }
    }

    #[test]
    fn illegal_transitions_are_rejected() {
        use CloneAttemptStatus::*;
        let illegal = [
            // Terminal states have no outgoing edge.
            (Ready, Cloning),
            (Ready, Failed),
            (Ready, Queued),
            (Ready, Cancelled),
            (Failed, Cloning),
            (Failed, Queued), // retry is a NEW attempt, not a transition
            (Failed, Ready),
            (Cancelled, Queued),
            (Cancelled, Cloning),
            (Cancelled, Ready),
            // Skipping a step.
            (Queued, Ready),
            (Queued, Failed),
            // No self-loops.
            (Queued, Queued),
            (Cloning, Cloning),
        ];
        for (from, to) in illegal {
            let err = from.try_transition(to).unwrap_err();
            assert_eq!(err.from, from);
            assert_eq!(err.to, to);
        }
    }

    #[test]
    fn terminal_states_are_flagged() {
        assert!(!CloneAttemptStatus::Queued.is_terminal());
        assert!(!CloneAttemptStatus::Cloning.is_terminal());
        assert!(CloneAttemptStatus::Ready.is_terminal());
        assert!(CloneAttemptStatus::Failed.is_terminal());
        assert!(CloneAttemptStatus::Cancelled.is_terminal());
    }

    #[test]
    fn clone_status_as_str_and_roundtrip() {
        assert_eq!(CloneStatus::None.as_str(), "none");
        assert_eq!(CloneStatus::Queued.as_str(), "queued");
        assert_eq!(CloneStatus::Cloning.as_str(), "cloning");
        assert_eq!(CloneStatus::Ready.as_str(), "ready");
        assert_eq!(CloneStatus::Failed.as_str(), "failed");
        for status in CloneStatus::ALL {
            let parsed: CloneStatus = status.as_str().parse().expect("round-trip");
            assert_eq!(parsed, status);
        }
        assert!("cancelled".parse::<CloneStatus>().is_err()); // not a project summary value
        assert!("bogus".parse::<CloneStatus>().is_err());
    }

    #[test]
    fn clone_status_from_attempt_denormalizes() {
        use CloneAttemptStatus as A;
        assert_eq!(CloneStatus::from_attempt(None), CloneStatus::None);
        // The carry-forward rule: a cancelled attempt's project shows no clone.
        assert_eq!(CloneStatus::from_attempt(Some(A::Cancelled)), CloneStatus::None);
        assert_eq!(CloneStatus::from_attempt(Some(A::Queued)), CloneStatus::Queued);
        assert_eq!(CloneStatus::from_attempt(Some(A::Cloning)), CloneStatus::Cloning);
        assert_eq!(CloneStatus::from_attempt(Some(A::Ready)), CloneStatus::Ready);
        assert_eq!(CloneStatus::from_attempt(Some(A::Failed)), CloneStatus::Failed);
    }

    // -- Drift guard: enums vs migration CHECK lists ------------------------

    /// The M0 migration text, included so this test fails if the Rust enums and
    /// the SQL CHECK constraints ever drift apart.
    const MIGRATION_068: &str = include_str!("../../../db/migrations/068_project_clone.sql");

    /// Extract the `IN (...)` value list of the named CHECK constraint from the
    /// migration SQL, returning the quoted values in order.
    ///
    /// The constraint name can appear more than once (e.g. an idempotency-guard
    /// `information_schema` lookup, then the real `ADD CONSTRAINT ... CHECK`), so
    /// we anchor on the FIRST occurrence that is followed by an `IN (` group and
    /// parse the values strictly out of that parenthesized list.
    fn check_values(sql: &str, constraint_name: &str) -> Vec<String> {
        let mut search_from = 0;
        loop {
            let anchor = sql[search_from..]
                .find(constraint_name)
                .map(|rel| search_from + rel)
                .unwrap_or_else(|| panic!("constraint {constraint_name} with an IN(...) list not found in migration"));
            let rest = &sql[anchor..];

            // Find the `IN (` belonging to this constraint, but stop at the next
            // statement boundary so we never run into an unrelated later list.
            let stmt_end = rest.find(';').unwrap_or(rest.len());
            if let Some(in_rel) = rest[..stmt_end].find("IN (").or_else(|| rest[..stmt_end].find("IN(")) {
                let after_in = &rest[in_rel..];
                let open = after_in.find('(').expect("IN list opening paren");
                let close = after_in[open..].find(')').expect("IN list closing paren") + open;
                let inner = &after_in[open + 1..close];
                return inner
                    .split(',')
                    .filter_map(|chunk| {
                        let t = chunk.trim();
                        let t = t.strip_prefix('\'')?;
                        let t = t.strip_suffix('\'')?;
                        Some(t.to_string())
                    })
                    .collect();
            }

            // This occurrence had no IN(...) before the statement end; advance.
            search_from = anchor + constraint_name.len();
        }
    }

    #[test]
    fn attempt_status_enum_matches_migration_check() {
        let mut sql_values = check_values(MIGRATION_068, "project_clone_attempts_status_check");
        sql_values.sort();
        let mut enum_values: Vec<String> = CloneAttemptStatus::ALL.iter().map(|s| s.as_str().to_string()).collect();
        enum_values.sort();
        assert_eq!(
            enum_values, sql_values,
            "CloneAttemptStatus::as_str() set must equal project_clone_attempts_status_check"
        );
    }

    #[test]
    fn clone_status_enum_matches_migration_check() {
        let mut sql_values = check_values(MIGRATION_068, "projects_clone_status_check");
        sql_values.sort();
        let mut enum_values: Vec<String> = CloneStatus::ALL.iter().map(|s| s.as_str().to_string()).collect();
        enum_values.sort();
        assert_eq!(enum_values, sql_values, "CloneStatus::as_str() set must equal projects_clone_status_check");
    }

    // -- WorkspaceDirName ----------------------------------------------------

    #[test]
    fn workspace_dir_name_derives_safe_names() {
        assert_eq!(WorkspaceDirName::derive("My Project").as_str(), "my-project");
        assert_eq!(WorkspaceDirName::derive("Engineering").as_str(), "engineering");
        assert_eq!(WorkspaceDirName::derive("A &&& B").as_str(), "a-b");
        assert_eq!(WorkspaceDirName::derive("!!foo__bar!!").as_str(), "foo-bar");
        assert_eq!(WorkspaceDirName::derive("---hi---").as_str(), "hi");
        assert_eq!(WorkspaceDirName::derive("  spaces  ").as_str(), "spaces");
    }

    #[test]
    fn workspace_dir_name_neutralizes_traversal_and_reserved() {
        // Traversal tokens / slashes / reserved names never survive.
        for input in ["../etc", "a/b/c", "..", ".", ".git", "", "   ", "/", "////"] {
            let derived = WorkspaceDirName::derive(input);
            let s = derived.as_str();
            assert!(!s.is_empty(), "{input:?} -> empty");
            assert!(!s.contains('/'), "{input:?} -> {s} contains /");
            assert!(!s.contains('\\'), "{input:?} -> {s} contains backslash");
            assert!(!s.contains(".."), "{input:?} -> {s} contains ..");
            assert!(!RESERVED_DIR_NAMES.contains(&s), "{input:?} -> reserved {s}");
            assert!(
                s.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'),
                "{input:?} -> {s} has unsafe chars"
            );
        }
        // Inputs that reduce to nothing fall back to the safe default.
        assert_eq!(WorkspaceDirName::derive("..").as_str(), "project");
        assert_eq!(WorkspaceDirName::derive("").as_str(), "project");
        // ".git" loses its leading dot and becomes the harmless dir name "git"
        // (it can never BE the literal ".git" once dots are stripped). The
        // reserved-name fallback guards the pre-filtered path as defense in
        // depth; here the filtered result "git" is already safe.
        assert_eq!(WorkspaceDirName::derive(".git").as_str(), "git");
    }

    #[test]
    fn workspace_dir_name_handles_unicode_and_symbols() {
        // Pure non-ascii / emoji / symbols collapse to the fallback.
        assert_eq!(WorkspaceDirName::derive("日本語").as_str(), "project");
        assert_eq!(WorkspaceDirName::derive("🚀🚀🚀").as_str(), "project");
        assert_eq!(WorkspaceDirName::derive("!@#$%^&*()").as_str(), "project");
        // Mixed: ascii survives, the rest collapses to single dashes.
        assert_eq!(WorkspaceDirName::derive("hello 世界 world").as_str(), "hello-world");
        let leading = WorkspaceDirName::derive("...leading.dots");
        assert_eq!(leading.as_str(), "leading-dots");
    }

    #[test]
    fn workspace_dir_name_caps_length() {
        let long = "a".repeat(200);
        let derived = WorkspaceDirName::derive(&long);
        assert_eq!(derived.as_str().len(), WORKSPACE_DIR_NAME_MAX);
        // A cap that lands mid-dash must not leave a trailing dash.
        let dashed = format!("{}{}", "a".repeat(63), " bbb");
        let derived = WorkspaceDirName::derive(&dashed);
        assert!(!derived.as_str().ends_with('-'));
        assert!(derived.as_str().len() <= WORKSPACE_DIR_NAME_MAX);
    }

    #[test]
    fn resolve_under_places_name_inside_root() {
        let root = Path::new("/var/agentforge/projects");
        let name = WorkspaceDirName::derive("My Repo");
        let resolved = name.resolve_under(root).unwrap();
        assert_eq!(resolved, Path::new("/var/agentforge/projects/my-repo"));
        assert!(resolved.starts_with(root));
    }

    #[test]
    fn resolve_under_does_not_require_existence() {
        // The root and target both do NOT exist on disk; resolution still works
        // because we never canonicalize the (non-existent) target.
        let root = Path::new("/nonexistent-root-xyz/projects");
        let name = WorkspaceDirName::derive("brand new");
        let resolved = name.resolve_under(root).unwrap();
        assert_eq!(resolved, Path::new("/nonexistent-root-xyz/projects/brand-new"));
    }

    #[test]
    fn resolve_under_rejects_crafted_escapes() {
        let root = Path::new("/var/agentforge/projects");
        // Construct adversarial names that bypass `derive` to prove the
        // structural containment check itself rejects escapes.
        for raw in ["../escape", "..", "a/b", "/abs", "a\\b", ".git", ""] {
            let crafted = WorkspaceDirName { value: raw.to_string() };
            assert!(crafted.resolve_under(root).is_err(), "crafted {raw:?} should be rejected by resolve_under");
        }
    }

    // -- Error class + redaction --------------------------------------------

    #[test]
    fn redact_strips_url_credentials() {
        let blob = "fatal: unable to access 'https://alice:ghp_supersecrettoken1234@github.com/x.git'";
        let out = redact(blob);
        assert!(!out.contains("ghp_supersecrettoken1234"), "token leaked: {out}");
        assert!(!out.contains("alice:"), "username leaked: {out}");
        assert!(out.contains("github.com"), "host should survive: {out}");
    }

    #[test]
    fn redact_strips_standalone_tokens() {
        let blob = "token ghp_abcdefghijklmnopqrstuvwxyz012345 was rejected";
        let out = redact(blob);
        assert!(!out.contains("ghp_abcdefghijklmnopqrstuvwxyz012345"), "{out}");
        assert!(out.contains("[REDACTED]"), "{out}");
        assert!(out.contains("was rejected"), "prose should survive: {out}");

        let glab = "remote: HTTP Basic: Access denied glpat-AbCdEf0123456789xyz now";
        let out = redact(glab);
        assert!(!out.contains("glpat-AbCdEf0123456789xyz"), "{out}");
    }

    #[test]
    fn redact_collapses_whitespace_and_truncates() {
        let blob = format!("error:{}done", " ".repeat(50));
        let out = redact(&blob);
        assert_eq!(out, "error: done");

        let huge = "x ".repeat(10_000); // ~20 KB
        let out = redact(&huge);
        assert!(out.chars().count() <= ERROR_MESSAGE_MAX, "len {}", out.chars().count());
    }

    #[test]
    fn redact_preserves_short_non_secret_words() {
        let blob = "error cloning repo: branch main not found";
        let out = redact(blob);
        assert_eq!(out, "error cloning repo: branch main not found");
    }

    #[test]
    fn classify_maps_representative_messages() {
        assert_eq!(
            CloneErrorClass::classify("fatal: Authentication failed for 'https://github.com/x'"),
            CloneErrorClass::Auth
        );
        assert_eq!(CloneErrorClass::classify("remote: Repository not found. fatal: 404"), CloneErrorClass::NotFound);
        assert_eq!(
            CloneErrorClass::classify("fatal: unable to access ...: Could not resolve host: github.com"),
            CloneErrorClass::Network
        );
        assert_eq!(CloneErrorClass::classify("ssh: connect ... Operation timed out"), CloneErrorClass::Timeout);
        assert_eq!(CloneErrorClass::classify("fatal: pack exceeds maximum allowed size"), CloneErrorClass::TooLarge);
        assert_eq!(CloneErrorClass::classify("some unexpected internal git crash"), CloneErrorClass::Internal);
    }

    #[test]
    fn clone_error_class_as_str_is_stable() {
        assert_eq!(CloneErrorClass::Auth.as_str(), "auth");
        assert_eq!(CloneErrorClass::NotFound.as_str(), "not_found");
        assert_eq!(CloneErrorClass::Network.as_str(), "network");
        assert_eq!(CloneErrorClass::Timeout.as_str(), "timeout");
        assert_eq!(CloneErrorClass::TooLarge.as_str(), "too_large");
        assert_eq!(CloneErrorClass::Internal.as_str(), "internal");
    }
}

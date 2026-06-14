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

use std::{fmt, path::Path, path::PathBuf, str::FromStr, sync::LazyLock};

use regex::Regex;
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
    /// "Latest" here means the attempt with the highest `attempt` number
    /// (`MAX(attempt)` for the project), NOT the most-recently-`updated_at` row.
    /// M5 must pass the genuinely-highest-numbered attempt: the
    /// `Cancelled -> None` collapse below assumes the cancelled row really is the
    /// last attempt, so that a project whose newest attempt was cancelled shows
    /// no active clone (a stale older `Ready`/`Failed` must not win).
    ///
    /// `None` (no attempt yet) and `Some(Cancelled)` both collapse to `None`:
    /// a project with no attempt, or whose latest attempt was cancelled,
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

/// An error-class string that does not match any known DB/API slug.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("unknown clone error class: {raw}")]
pub struct UnknownCloneErrorClass {
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
///
/// The field is private and there is no public struct literal: the ONLY way to
/// obtain a `WorkspaceDirName` outside this module is [`WorkspaceDirName::derive`]
/// (total, for fresh names) or [`WorkspaceDirName::parse`] (validating, for names
/// read back from the DB in M2). Both go through [`is_safe_dir_name`], so the
/// invariant "always a single safe path component" cannot be bypassed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceDirName {
    value: String,
}

/// The single safety predicate every `WorkspaceDirName` value must satisfy.
///
/// A safe name is non-empty, length-capped, all `[a-z0-9-]`, has no leading or
/// trailing `-`, and is not a reserved traversal/metadata token. This is the
/// shared invariant for both [`WorkspaceDirName::derive`] (which produces names
/// that satisfy it by construction) and [`WorkspaceDirName::parse`] (which
/// rejects names that do not). Keeping the check in one place stops the two
/// constructors from drifting.
fn is_safe_dir_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= WORKSPACE_DIR_NAME_MAX
        && value.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && !value.starts_with('-')
        && !value.ends_with('-')
        && !RESERVED_DIR_NAMES.contains(&value)
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
    ///
    /// The result always satisfies [`is_safe_dir_name`]. The reserved-name
    /// fallback is in practice unreachable given the `[a-z0-9-]` filter (a
    /// filtered run can never equal `.`, `..`, or `.git`, since `.` is dropped);
    /// it is kept as a forward guard so a future filter change cannot silently
    /// produce a reserved name. The byte-level `truncate(WORKSPACE_DIR_NAME_MAX)`
    /// is safe to do on a byte boundary only because `out` is ASCII-only here.
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

    /// Validate a directory name read back from the DB (M2 reads
    /// `projects.workspace_dir_name`, backfilled by migration 068 from OLD slugs
    /// that a weaker policy produced and that never checked for `.git`).
    ///
    /// Unlike [`derive`](Self::derive), this does NOT sanitize: it re-runs the
    /// full safety predicate ([`is_safe_dir_name`]) and rejects anything that is
    /// empty, over-length, contains a character outside `[a-z0-9-]` (so `/`,
    /// `\`, `.`, `..` are all rejected), has a leading/trailing `-`, or is a
    /// reserved name. This is the only validating constructor; M2 must use it
    /// rather than trusting the stored value.
    pub fn parse(value: &str) -> Result<WorkspaceDirName, PathEscape> {
        if is_safe_dir_name(value) {
            Ok(WorkspaceDirName { value: value.to_string() })
        } else {
            Err(PathEscape { name: value.to_string() })
        }
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
    ///
    /// PRECONDITION: the caller MUST pass an absolute, canonicalized `root`. The
    /// structural containment proof below is moot if `root` is relative or
    /// contains unresolved symlinks (a `..` inside a symlinked `root` could still
    /// escape on the real filesystem). M2 canonicalizes the projects root once at
    /// startup and passes that. The `debug_assert!` documents and enforces this
    /// in test/dev builds.
    pub fn resolve_under(&self, root: &Path) -> Result<PathBuf, PathEscape> {
        debug_assert!(root.is_absolute(), "resolve_under requires an absolute, canonicalized root");

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

impl FromStr for CloneErrorClass {
    type Err = UnknownCloneErrorClass;

    /// Parse the stored `error_class` slug back into the enum (M5 reads it back).
    /// Symmetric with [`CloneErrorClass::as_str`].
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "auth" => Ok(Self::Auth),
            "not_found" => Ok(Self::NotFound),
            "network" => Ok(Self::Network),
            "timeout" => Ok(Self::Timeout),
            "too_large" => Ok(Self::TooLarge),
            "internal" => Ok(Self::Internal),
            other => Err(UnknownCloneErrorClass { raw: other.to_string() }),
        }
    }
}

/// The marker substituted for any redacted secret run.
const REDACTED: &str = "[REDACTED]";

/// A clone error message that has passed through [`redact`].
///
/// This newtype is a type-level proof that a string was scrubbed of credentials:
/// the only public constructor is [`redact`], so a downstream milestone (M5/M6)
/// cannot accidentally persist raw `git` stderr into `error_message` — the
/// persistence boundary takes a `RedactedError`, and the only way to make one is
/// to pass raw stderr through the redactor. Treat the wrapped string as the
/// safe-to-display, safe-to-store value; the raw stderr stays server-side in
/// logs and is never wrapped here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RedactedError(String);

impl RedactedError {
    /// The redacted, safe-to-store message.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Consume into the owned redacted string (for persistence at the boundary).
    pub fn into_string(self) -> String {
        self.0
    }
}

impl fmt::Display for RedactedError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Redact secrets from a raw clone error blob before it is persisted/displayed.
///
/// Defense in depth, applied in order:
///   1. Scrub the VALUE of sensitive URL query params
///      (`?access_token=…`, `&token=…`, `private_token`, `api_key`, `password`,
///      `auth`, …) regardless of the surrounding delimiters.
///   2. Strip `userinfo@` from embedded `scheme://userinfo@host` authorities.
///   3. Scrub standalone secret-looking runs anywhere in the blob — provider
///      token prefixes (`ghp_…`, `glpat-…`, `github_pat_…`) and long opaque
///      `[A-Za-z0-9]{32,}` / hex runs — even when glued to quotes, parens,
///      commas, angle brackets, or slashes (NOT whitespace-tokenized).
///   4. Collapse whitespace and truncate to [`ERROR_MESSAGE_MAX`].
///
/// This is best-effort and biased toward over-redaction of long opaque runs
/// (real prose words are short; a 32+ char alphanumeric run is almost never
/// prose). It is NOT the primary control: the real guarantee is that a clone
/// credential never reaches `git` stderr in the first place (M3 writes the token
/// to an askpass/credential helper, M4 keeps it out of argv and the error path).
/// Redaction is the last line of defense for stderr that slips a token through.
pub fn redact(raw: &str) -> RedactedError {
    let mut s = redact_query_param_values(raw);
    s = redact_url_userinfo(&s);
    s = redact_token_like(&s);
    s = collapse_whitespace(&s);
    RedactedError(truncate_chars(&s, ERROR_MESSAGE_MAX))
}

/// Scrub the value of a sensitive URL query parameter regardless of the
/// delimiter that introduced it (`?` or `&`).
///
/// Matches `[?&]<param>=<value>` where `<value>` is a run of non-`&`,
/// non-whitespace, non-quote characters, and rewrites it to `<param>=[REDACTED]`.
/// This catches `…?access_token=ghp_…/'` shapes that the userinfo strip and the
/// whitespace-free token scan could otherwise miss when the token is glued to
/// path/quote characters.
fn redact_query_param_values(raw: &str) -> String {
    static QUERY_PARAM: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r#"(?i)([?&](?:access_token|token|private_token|api[_-]?key|x-access-token|password|auth)=)[^&\s"']+"#,
        )
        .expect("clone-redaction query-param regex must compile")
    });
    QUERY_PARAM.replace_all(raw, |caps: &regex::Captures<'_>| format!("{}{REDACTED}", &caps[1])).into_owned()
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
            // Copy this char verbatim (advance by one full UTF-8 char). A `str`
            // slice always starts on a char boundary, so `chars().next()` is
            // `Some`; `.expect` turns a future loop-advance regression into a
            // test-visible panic instead of a silent replacement char.
            let ch = raw[i..].chars().next().expect("Rust str is always valid UTF-8");
            out.push(ch);
            i += ch.len_utf8();
        }
    }

    out
}

/// Redact secret-looking runs anywhere in the blob, NOT whitespace-tokenized, so
/// a token glued to quotes / parens / commas / angle brackets / slashes is still
/// caught. Two passes, longest-match-first:
///
///   1. Known provider prefixes followed by the token body
///      (`ghp_|gho_|ghu_|ghs_|ghr_|github_pat_|glpat-` + `[A-Za-z0-9_-]+`).
///   2. High-entropy opaque runs: any `[A-Za-z0-9]{32,}` (covers hex too).
///
/// A run is bordered by any non-token-body character, so `'ghp_…'`, `(ghp_…)`,
/// `</ghp_…>`, and `tokens=ghp_…,other` all redact the secret and keep the rest.
fn redact_token_like(raw: &str) -> String {
    static PREFIXED_TOKEN: LazyLock<Regex> = LazyLock::new(|| {
        // Prefixed tokens: lower the body floor (>= 8) since the prefix already
        // signals a secret. `(?-i)` keeps the prefixes case-exact.
        Regex::new(r#"(?-i)\b(?:ghp_|gho_|ghu_|ghs_|ghr_|github_pat_|glpat-)[A-Za-z0-9_-]{8,}"#)
            .expect("clone-redaction prefixed-token regex must compile")
    });
    static OPAQUE_RUN: LazyLock<Regex> = LazyLock::new(|| {
        // Any opaque alnum run >= 32 (covers all-lowercase, digit-free, and
        // 40-hex-with-letters cases that a charset-class gate would miss).
        // `(?-i)` is irrelevant for the class but keeps intent explicit.
        Regex::new(r#"[A-Za-z0-9]{32,}"#).expect("clone-redaction opaque-run regex must compile")
    });
    let s = PREFIXED_TOKEN.replace_all(raw, REDACTED).into_owned();
    OPAQUE_RUN.replace_all(&s, REDACTED).into_owned()
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
        // Generated ALL × ALL sweep: every (from, to) pair that is NOT one of the
        // five legal edges (incl. terminal self-loops like `Ready -> Ready` and
        // backward edges like `Cloning -> Queued`) MUST be rejected. This cannot
        // drift out of date the way a hand-listed array can.
        use CloneAttemptStatus::*;
        const LEGAL: [(CloneAttemptStatus, CloneAttemptStatus); 5] =
            [(Queued, Cloning), (Queued, Cancelled), (Cloning, Ready), (Cloning, Failed), (Cloning, Cancelled)];

        for from in CloneAttemptStatus::ALL {
            for to in CloneAttemptStatus::ALL {
                if LEGAL.contains(&(from, to)) {
                    continue;
                }
                let err =
                    from.try_transition(to).expect_err(&format!("{from} -> {to} must be illegal (not a legal edge)"));
                assert_eq!(err.from, from);
                assert_eq!(err.to, to);
            }
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
    ///
    /// ASSUMPTION (so a future migration reformat does not silently break this):
    /// the real constraint is written as `ADD CONSTRAINT <name> CHECK (... IN (
    /// 'a', 'b', ... ))` on a SINGLE statement (terminated by `;`), with the
    /// allowed values as single-quoted literals inside the FIRST `IN (...)` group
    /// that follows the constraint name before that `;`. If migration 068 is ever
    /// rewritten to use an enum type, a lookup table, or a multi-statement form,
    /// update this parser (and these tests will fail loudly first).
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

    #[test]
    fn workspace_dir_name_parse_validates_stored_values() {
        // T1: the validating constructor accepts already-safe names...
        assert_eq!(WorkspaceDirName::parse("my-project").unwrap().as_str(), "my-project");
        assert_eq!(WorkspaceDirName::parse("a").unwrap().as_str(), "a");
        assert_eq!(WorkspaceDirName::parse("abc123").unwrap().as_str(), "abc123");
        // ...and rejects every unsafe shape (it does NOT sanitize like derive).
        for bad in [
            "",                                      // empty
            "..",                                    // traversal
            ".",                                     // dot
            ".git",                                  // reserved metadata
            "a/b",                                   // slash
            "a\\b",                                  // backslash
            "a..b",                                  // embedded `..` -> contains `.`
            "-lead",                                 // leading dash
            "trail-",                                // trailing dash
            "Upper",                                 // uppercase
            "has space",                             // whitespace
            "under_score",                           // disallowed char
            &"a".repeat(WORKSPACE_DIR_NAME_MAX + 1), // over-length
        ] {
            assert!(WorkspaceDirName::parse(bad).is_err(), "parse({bad:?}) should be rejected");
        }
    }

    #[test]
    fn derive_output_always_passes_parse() {
        // Anything derive() produces must satisfy parse() — the two constructors
        // share is_safe_dir_name and cannot drift.
        for input in ["My Project", "..", "🚀", "", ".git", &"x".repeat(300), "  spaces  ", "---hi---"] {
            let derived = WorkspaceDirName::derive(input);
            assert_eq!(
                WorkspaceDirName::parse(derived.as_str()).expect("derive output must re-parse").as_str(),
                derived.as_str(),
                "derive({input:?}) -> {} failed re-parse",
                derived.as_str()
            );
        }
    }

    // -- Error class + redaction --------------------------------------------

    /// Helper: redact and return the wrapped string for substring assertions.
    fn redacted(raw: &str) -> String {
        redact(raw).into_string()
    }

    #[test]
    fn redact_strips_url_credentials() {
        let blob = "fatal: unable to access 'https://alice:ghp_supersecrettoken1234@github.com/x.git'";
        let out = redacted(blob);
        assert!(!out.contains("ghp_supersecrettoken1234"), "token leaked: {out}");
        assert!(!out.contains("alice:"), "username leaked: {out}");
        assert!(out.contains("github.com"), "host should survive: {out}");
    }

    #[test]
    fn redact_strips_standalone_tokens() {
        let blob = "token ghp_abcdefghijklmnopqrstuvwxyz012345 was rejected";
        let out = redacted(blob);
        assert!(!out.contains("ghp_abcdefghijklmnopqrstuvwxyz012345"), "{out}");
        assert!(out.contains("[REDACTED]"), "{out}");
        assert!(out.contains("was rejected"), "prose should survive: {out}");

        let glab = "remote: HTTP Basic: Access denied glpat-AbCdEf0123456789xyz now";
        let out = redacted(glab);
        assert!(!out.contains("glpat-AbCdEf0123456789xyz"), "{out}");
    }

    #[test]
    fn redact_collapses_whitespace_and_truncates() {
        let blob = format!("error:{}done", " ".repeat(50));
        let out = redacted(&blob);
        assert_eq!(out, "error: done");

        let huge = "x ".repeat(10_000); // ~20 KB
        let out = redacted(&huge);
        assert!(out.chars().count() <= ERROR_MESSAGE_MAX, "len {}", out.chars().count());
    }

    #[test]
    fn redact_preserves_short_non_secret_words() {
        let blob = "error cloning repo: branch main not found";
        let out = redacted(blob);
        assert_eq!(out, "error cloning repo: branch main not found");
    }

    // -- Adversarial redaction shapes (S1/S2/S3) ----------------------------
    //
    // A single GitHub PAT body, reused across delimiter shapes. 36 alnum chars
    // after the `ghp_` prefix — a real-world GitHub token length.
    const SECRET_BODY: &str = "ghp_abcdefghijklmnopqrstuvwxyz0123456789";

    #[test]
    fn redact_scrubs_token_in_query_string() {
        // S1: the value of a sensitive query param, glued to a trailing slash and
        // quote, must be scrubbed even though it is not whitespace-delimited.
        let blob =
            format!("fatal: unable to access 'https://github.com/o/r?access_token={SECRET_BODY}/': The requested URL");
        let out = redacted(&blob);
        assert!(!out.contains(SECRET_BODY), "query-string token leaked: {out}");
        assert!(out.contains("github.com"), "host should survive: {out}");
    }

    #[test]
    fn redact_scrubs_other_sensitive_query_params() {
        for param in ["token", "private_token", "api_key", "api-key", "x-access-token", "password", "auth"] {
            let blob = format!("https://h/r?{param}={SECRET_BODY}");
            let out = redacted(&blob);
            assert!(!out.contains(SECRET_BODY), "param {param} leaked: {out}");
        }
    }

    #[test]
    fn redact_scrubs_tokens_glued_to_non_whitespace_delimiters() {
        // S2: leading/trailing quotes, parens, commas, angle/slash all defeated
        // the old whitespace-tokenized scan. Every shape must now redact.
        let shapes = [
            format!("'{SECRET_BODY}'"),            // quote-glued
            format!("\"{SECRET_BODY}\""),          // double-quote
            format!("({SECRET_BODY})"),            // paren
            format!("tokens={SECRET_BODY},other"), // comma list
            format!("</{SECRET_BODY}>"),           // angle + slash
            format!("[{SECRET_BODY}]"),            // bracket
            format!("=({SECRET_BODY})"),           // equals + paren
        ];
        for shape in shapes {
            let out = redacted(&shape);
            assert!(!out.contains(SECRET_BODY), "secret leaked from shape {shape:?}: {out}");
        }
    }

    #[test]
    fn redact_scrubs_low_charset_opaque_tokens() {
        // S3: tokens the old base64 gate (upper AND lower AND digit) let slip.
        // 40-char hex with letters:
        let hex40 = "a1b2c3d4e5f60718293a4b5c6d7e8f90a1b2c3d4";
        // 40-char all-lowercase (no digits):
        let lower40 = "abcdefghijklmnopqrstuvwxyzabcdefghijklmn";
        // 40-char digit-free mixed case:
        let nodigit40 = "AbCdEfGhIjKlMnOpQrStUvWxYzAbCdEfGhIjKlMn";
        for secret in [hex40, lower40, nodigit40] {
            let blob = format!("remote: rejected credential {secret} please retry");
            let out = redacted(&blob);
            assert!(!out.contains(secret), "opaque token leaked: {out}");
            assert!(out.contains("[REDACTED]"), "no marker: {out}");
            assert!(out.contains("please retry"), "prose should survive: {out}");
        }
    }

    #[test]
    fn redact_scrubs_userinfo_token_form() {
        let blob = format!("https://user:{SECRET_BODY}@host/path failed");
        let out = redacted(&blob);
        assert!(!out.contains(SECRET_BODY), "userinfo token leaked: {out}");
        assert!(out.contains("host"), "host should survive: {out}");
    }

    #[test]
    fn redact_handles_large_blob_without_leaking() {
        // ~20 KB blob with a secret buried in the middle; bounded output, no leak.
        let mut blob = "x".repeat(10_000);
        blob.push_str(&format!(" {SECRET_BODY} "));
        blob.push_str(&"y".repeat(10_000));
        let out = redacted(&blob);
        assert!(!out.contains(SECRET_BODY), "buried token leaked");
        assert!(out.chars().count() <= ERROR_MESSAGE_MAX);
    }

    #[test]
    fn redacted_error_is_a_proof_type() {
        // T3: the only way to get a RedactedError is through redact(); as_str /
        // Display expose the scrubbed value.
        let re = redact("token ghp_abcdefghijklmnopqrstuvwxyz012345 bad");
        assert!(!re.as_str().contains("ghp_abcdefghijklmnopqrstuvwxyz012345"));
        assert_eq!(re.as_str(), re.to_string());
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
    fn classify_precedence_is_deterministic() {
        // When a blob matches multiple signatures, the documented order wins:
        // timeout before network, auth before not-found.
        assert_eq!(
            CloneErrorClass::classify("fatal: could not resolve host: github.com (operation timed out)"),
            CloneErrorClass::Timeout,
            "timeout must outrank network"
        );
        assert_eq!(
            CloneErrorClass::classify("remote: Permission denied. Repository not found."),
            CloneErrorClass::Auth,
            "auth must outrank not-found"
        );
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

    #[test]
    fn clone_error_class_roundtrips_str() {
        for class in [
            CloneErrorClass::Auth,
            CloneErrorClass::NotFound,
            CloneErrorClass::Network,
            CloneErrorClass::Timeout,
            CloneErrorClass::TooLarge,
            CloneErrorClass::Internal,
        ] {
            let parsed: CloneErrorClass = class.as_str().parse().expect("round-trip parse");
            assert_eq!(parsed, class);
        }
        assert!("bogus".parse::<CloneErrorClass>().is_err());
        assert!("".parse::<CloneErrorClass>().is_err());
        // Case-sensitive: DB slugs are lowercase only.
        assert!("Auth".parse::<CloneErrorClass>().is_err());
    }
}

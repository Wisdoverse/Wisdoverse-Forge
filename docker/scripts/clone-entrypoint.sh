#!/usr/bin/env bash
# =============================================================================
# Wisdoverse Forge — ephemeral clone container entrypoint
# =============================================================================
# Single-purpose, fail-fast script that clones ONE git repository into a mounted
# staging directory and reports the result. Runs inside the minimal
# `agentforge-clone` image (git + ca-certificates + openssh-client + tini only).
#
# Unlike agent-entrypoint.sh (which must keep a CLI alive on partial failure and
# therefore runs without `set -e`), this script does exactly one thing, so
# fail-fast is correct: any unexpected error aborts with a non-zero exit and the
# server-side worker (M5) reads the exit code + stderr.
#
# SECURITY CONTRACT:
#   - The credential NEVER appears in git's argv, in the URL, or in stderr.
#   - The token is delivered ONLY via a read-only mounted secret file at
#     /run/secrets/git-credential (mode 0400) and is handed to git through a
#     one-shot git credential helper scoped to the target host. git reads the
#     token over the helper's stdout pipe — it is never on a command line and
#     never echoed by this script.
#   - This script must never print the contents of the secret file.
#
# Inputs (environment):
#   CLONE_URL       (required) HTTPS clone URL, e.g. https://github.com/org/repo.git
#   CLONE_DEST      (required) staging mount dir; the repo is cloned to $CLONE_DEST/repo
#   CLONE_PROVIDER  (optional) github | gitlab | "" — informational only in v1
#
# Credential (optional):
#   /run/secrets/git-credential  read-only file (mode 0400). The FIRST non-blank
#   line is the credential. Supported forms (the helper splits on the FIRST ':'):
#     - "x-access-token:<token>"   GitHub app/installation + PAT-over-HTTPS form
#     - "oauth2:<token>"           GitLab OAuth2 form
#     - "<user>:<token>"           explicit username:token
#     - "<token>"                  bare token, NO colon -> username defaults to
#                                  x-access-token
#   CONTRACT for M6 (which writes the secret bytes): a bare token MUST NOT contain
#   a ':' — a `:` in the credential is ALWAYS treated as the user:pass separator,
#   so a token that itself contains a ':' would be mis-split into user/pass. M6
#   must therefore emit one of the explicit colon-form indicators above
#   (`x-access-token:<token>` / `oauth2:<token>` / `<user>:<token>`) whenever the
#   token could contain a ':' — never a bare `<token>` with an embedded colon.
#   If the file is ABSENT, the clone proceeds unauthenticated (public repos). If
#   the file is PRESENT but empty/blank, the clone is REFUSED (fail 3) rather than
#   silently downgraded to an anonymous clone (which would mask a server-side
#   credential-injection bug for a public repo).
#
# Outputs:
#   On success: writes $CLONE_DEST/.clone-result.json with branch/head_sha/bytes,
#               exits 0.
#   On failure: prints git's stderr (NOT the secret), exits non-zero.
#
# NOT this script's responsibility (the M5 worker owns these): the per-clone
# resource limits (disk/CPU/PID), the disk-exhaustion + "too large" classification
# of git's exit 128, and the `agentforge.project_clone=<attempt_id>` container
# label used for reaping orphaned clone containers. This entrypoint only clones,
# validates inputs, and reports an exit code + result file.
# =============================================================================

set -o pipefail

# --- structured logging (never prints secrets) ------------------------------
log() { printf 'clone-entrypoint: %s\n' "$*" >&2; }
fail() {
  # $1 = exit code, rest = message. Message is operator-facing; the worker
  # redacts before persisting. Never include the secret here.
  local code="$1"; shift
  log "ERROR: $*"
  exit "$code"
}

SECRET_FILE="/run/secrets/git-credential"
# Internal helper path; written to a private, exec-permitted location at runtime.
CRED_HELPER="/tmp/agentforge-clone-credential-helper.sh"
# One-shot contract: the helper script must not outlive this process. Remove it on
# EVERY exit path (success, fail, or signal). It is only ever WRITTEN inside the
# secret-present branch, so `rm -f` is a harmless no-op when it was never created.
trap 'rm -f "$CRED_HELPER"' EXIT

# --- validate required inputs -----------------------------------------------
[ -n "${CLONE_URL:-}" ]  || fail 2 "CLONE_URL is required"
[ -n "${CLONE_DEST:-}" ] || fail 2 "CLONE_DEST is required"

# Defense-in-depth: only HTTPS is supported in v1 (the server already enforces
# this at parse time; re-assert here so a misconfigured caller fails cleanly and
# the SSH/credential-helper path is never reached for a non-https URL).
case "$CLONE_URL" in
  https://*) : ;;
  *) fail 2 "CLONE_URL must be an https:// URL" ;;
esac

# CRITICAL defense-in-depth: the URL must NEVER carry embedded credentials. The
# Rust ProjectRepositoryUrl::parse gate already rejects any `userinfo@` authority,
# but re-assert here so even a future caller that bypasses the Rust gate cannot
# leak a `user:token@host` into git's argv / this container's env (`docker
# inspect`) / /proc/<pid>/cmdline / git stderr. Credentials come ONLY from the
# mounted secret. (An `@` later in the path is not credentials, but the upstream
# gate forbids `@` in the authority anyway, so a blanket reject here is safe and
# strictly tighter; a legitimate clone URL has no `@`.)
case "$CLONE_URL" in
  *@*) fail 2 "CLONE_URL must not contain embedded credentials" ;;
esac

# CLONE_DEST must exist (it is the bind-mounted staging dir). Refuse to clone on
# top of an existing repo dir so a partial/aborted prior run can't be mistaken
# for a fresh clone.
[ -d "$CLONE_DEST" ] || fail 2 "CLONE_DEST '$CLONE_DEST' is not a directory (expected the staging mount)"
REPO_DIR="$CLONE_DEST/repo"
if [ -e "$REPO_DIR" ]; then
  fail 2 "target '$REPO_DIR' already exists — refusing to clone over it"
fi

log "cloning into $REPO_DIR (provider=${CLONE_PROVIDER:-unknown})"

# --- derive the target host for host-scoped credential config ----------------
# Strip scheme, then path/port, leaving the bare host. There is no userinfo to
# strip: a CLONE_URL containing '@' was already rejected above. Used only to scope
# the credential helper to this host; it is not a security control on its own (the
# restricted egress network is — see spec §10).
url_no_scheme="${CLONE_URL#https://}"
TARGET_HOST="${url_no_scheme%%/*}"
TARGET_HOST="${TARGET_HOST%%:*}"
[ -n "$TARGET_HOST" ] || fail 2 "could not derive host from CLONE_URL"

# --- assemble git -c credential args (host-scoped, one-shot helper) ----------
# GIT_CRED_ARGS is populated ONLY when a secret file is present. When the repo is
# public (no secret mounted) the helper is never configured and git clones
# anonymously.
GIT_CRED_ARGS=()
if [ -f "$SECRET_FILE" ]; then
  if [ ! -r "$SECRET_FILE" ]; then
    fail 3 "credential secret present at $SECRET_FILE but not readable (check mount mode 0400 + container uid)"
  fi
  # A PRESENT secret must yield a usable credential. If it is empty/whitespace-only
  # the helper would emit nothing and (with GIT_TERMINAL_PROMPT=0) git would fall
  # back to an ANONYMOUS clone — which SUCCEEDS for a public repo and silently
  # masks a server-side credential-injection bug. Refuse loudly instead. (A truly
  # ABSENT secret -> anonymous clone is fine and intended for public repos; only a
  # present-but-unusable secret is an error.) Validate the first non-blank line
  # here without echoing it. `grep` exit 1 (no match) is fine; only a non-blank
  # match passes.
  if ! grep -q -m1 -v '^[[:space:]]*$' "$SECRET_FILE" 2>/dev/null; then
    fail 3 "credential secret present but empty/blank — refusing to silently clone anonymously"
  fi
  log "credential secret detected — configuring one-shot host-scoped helper for $TARGET_HOST"

  # The credential helper is a tiny script that, on git's `get` request, emits
  # username/password parsed from the mounted secret. git invokes it ONLY for the
  # scoped host (credential.https://<host>.helper) and ONLY for `get`; for store/
  # erase it does nothing. The token flows helper-stdout -> git over a pipe, never
  # via argv/stderr. The helper reads the secret file path from the environment so
  # the path (not the token) is all that appears in `git -c` config.
  # Guard the write: a full /tmp would otherwise produce a truncated/empty helper
  # and a confusing git exit 128 later. Fail fast (exit 3) with a clear message.
  cat > "$CRED_HELPER" <<'HELPER_EOF' || fail 3 "failed to write credential helper (is /tmp full?)"
#!/usr/bin/env bash
# One-shot git credential helper for the ephemeral clone container.
# Responds only to the `get` operation; reads the token from the mounted secret.
set -o pipefail
op="$1"
[ "$op" = "get" ] || exit 0

secret_file="${AGENTFORGE_CLONE_SECRET_FILE:?secret file path not provided}"
[ -r "$secret_file" ] || exit 1

# Read the first non-empty line of the secret. The split is on the FIRST ':'.
# Supported forms (see the entrypoint header CONTRACT — M6 must honor it):
#   x-access-token:<token>   -> user=x-access-token (GitHub PAT/app form)
#   oauth2:<token>           -> user=oauth2          (GitLab OAuth2 form)
#   <username>:<token>       -> explicit username
#   <token>  (NO colon)      -> username defaults to x-access-token
# NOTE: a bare token containing a ':' would be mis-split into user:pass — M6 must
# never emit a colon-bearing bare token; it must use a colon-form indicator.
raw="$(grep -m1 -v '^[[:space:]]*$' "$secret_file" 2>/dev/null)"
[ -n "$raw" ] || exit 1

case "$raw" in
  *:*)
    cred_user="${raw%%:*}"
    cred_pass="${raw#*:}"
    ;;
  *)
    cred_user="x-access-token"
    cred_pass="$raw"
    ;;
esac

# Emit the git credential protocol response. The token is written to stdout,
# which git consumes over a pipe — it never reaches a terminal or a log.
printf 'username=%s\n' "$cred_user"
printf 'password=%s\n' "$cred_pass"
HELPER_EOF
  chmod 700 "$CRED_HELPER" || fail 3 "failed to install credential helper"
  export AGENTFORGE_CLONE_SECRET_FILE="$SECRET_FILE"

  # Scope the helper to the exact target host so it cannot fire for a redirect to
  # another host. Leading empty `helper=` clears any inherited helper first.
  # useHttpPath=false keeps the credential host-wide (not path-specific), matching
  # how the token is issued.
  GIT_CRED_ARGS+=(
    -c "credential.https://${TARGET_HOST}.helper="
    -c "credential.https://${TARGET_HOST}.helper=${CRED_HELPER}"
    -c "credential.https://${TARGET_HOST}.useHttpPath=false"
  )
else
  log "no credential secret at $SECRET_FILE — cloning anonymously (public repo)"
fi

# --- run the clone -----------------------------------------------------------
# Full history (a dev project needs it — no --depth). No recursive submodules.
# LFS smudge skipped by default. GIT_TERMINAL_PROMPT=0 makes a missing/bad
# credential fail fast instead of hanging on an interactive prompt.
#
# Defense-in-depth: clear any inherited askpass program. GIT_TERMINAL_PROMPT=0
# suppresses the interactive TTY prompt, but git/ssh will still invoke a
# GIT_ASKPASS / SSH_ASKPASS helper if one is set in the environment — which could
# bypass the no-prompt guarantee and source a credential from outside the mounted
# secret. Unset both so the ONLY credential path is the host-scoped helper above.
unset GIT_ASKPASS SSH_ASKPASS
log "starting git clone (full history, no submodules, LFS smudge skipped)"
# `-c credential.helper=` first clears any global/system helper chain (e.g. a
# baked-in store), so ONLY the host-scoped one-shot helper in GIT_CRED_ARGS can
# fire — and only for the target host. The clearing entry must precede the
# host-scoped helper so the host scope is not also wiped.
if GIT_LFS_SKIP_SMUDGE=1 GIT_TERMINAL_PROMPT=0 \
   git -c credential.helper= \
     "${GIT_CRED_ARGS[@]}" \
     clone --no-recurse-submodules "$CLONE_URL" "$REPO_DIR"; then
  log "clone succeeded"
else
  clone_rc=$?
  # git already wrote its own error to stderr (worker redacts before storing).
  fail "$clone_rc" "git clone failed (exit $clone_rc)"
fi

# --- collect result metadata -------------------------------------------------
# Resolved branch: prefer the current branch name; fall back to the symbolic ref.
resolved_branch="$(git -C "$REPO_DIR" branch --show-current 2>/dev/null)"
if [ -z "$resolved_branch" ]; then
  resolved_branch="$(git -C "$REPO_DIR" symbolic-ref --short -q HEAD 2>/dev/null || true)"
fi

head_sha="$(git -C "$REPO_DIR" rev-parse HEAD 2>/dev/null)"
[ -n "$head_sha" ] || fail 4 "clone reported success but HEAD could not be resolved"

# Byte count of the cloned tree (best-effort; 0 if du is unavailable).
bytes="$(du -sb "$REPO_DIR" 2>/dev/null | awk '{print $1}')"
case "$bytes" in
  ''|*[!0-9]*) bytes=0 ;;
esac

# --- write the result file (JSON) --------------------------------------------
# branch may legitimately be empty (detached HEAD on a tag/commit); emit "" then.
RESULT_FILE="$CLONE_DEST/.clone-result.json"
RESULT_TMP="$CLONE_DEST/.clone-result.json.tmp"

# Minimal JSON string escaping for the branch field (backslash + double-quote).
escape_json() {
  printf '%s' "$1" | sed 's/\\/\\\\/g; s/"/\\"/g'
}
branch_escaped="$(escape_json "$resolved_branch")"

if printf '{"branch":"%s","head_sha":"%s","bytes":%s}\n' \
     "$branch_escaped" "$head_sha" "$bytes" > "$RESULT_TMP" \
   && mv -f "$RESULT_TMP" "$RESULT_FILE"; then
  log "wrote result: branch='${resolved_branch}' head_sha=${head_sha} bytes=${bytes}"
else
  rm -f "$RESULT_TMP" 2>/dev/null
  fail 4 "clone succeeded but failed to write result file $RESULT_FILE"
fi

log "done"
exit 0

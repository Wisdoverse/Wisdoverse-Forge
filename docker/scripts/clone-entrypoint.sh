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
#     /run/secrets/git-credential and is handed to git through a
#     one-shot git credential helper scoped to the target host. git reads the
#     token over the helper's stdout pipe — it is never on a command line and
#     never echoed by this script.
#   - This script must never print the contents of the secret file.
#
# Inputs (environment):
#   CLONE_URL       (required) HTTPS clone URL, e.g. https://github.com/org/repo.git
#   CLONE_DEST      (required) staging mount dir; the repo is cloned to $CLONE_DEST/repo
#   CLONE_PROVIDER  (optional) github | gitlab | "" — informational only in v1
#   CLONE_MAX_BYTES (optional) hard cap on the cloned tree size in bytes. A
#                   background watchdog aborts the clone (exit 5) if exceeded. The
#                   M4 runtime always sets it (default 2 GiB); empty/unset disables
#                   the watchdog (NOT recommended outside tests).
#   CLONE_MIN_FREE_BYTES (optional) free-space floor for the staging filesystem
#                   checked BEFORE the clone (exit 5 if below). Defaults to
#                   CLONE_MAX_BYTES (need at least room for the cap) or, if that is
#                   unset, 64 MiB.
#
# Exit codes (the M5 worker maps these):
#   0  success (+ .clone-result.json)   2  bad input / SSRF-blocked host
#   3  credential/helper error          4  post-clone metadata/result-file error
#   5  disk guard: too large OR insufficient free space (→ TooLarge outcome)
#   128 (git) auth/not-found/transport
#
# Credential (optional):
#   /run/secrets/git-credential  read-only file (mode 0644 inside a backend-only
#   0700 secret root; see the M4 runtime docs for why not 0400). The FIRST non-blank
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
# Defense-in-depth, NOT the primary control (see the M4 runtime module docs):
#   - SSRF: a best-effort host pre-resolve below refuses a CLONE_URL whose host
#     resolves to loopback/RFC1918/link-local/metadata BEFORE git connects. This
#     is best-effort vs DNS-rebinding (the host can re-resolve at git's connect
#     time); the REAL control is the deploy-layer egress firewall on the clone
#     egress network (docs/runbooks/clone-egress-firewall.md). M8 adds the
#     fails-closed integration test.
#   - Disk: a free-space preflight + a background size watchdog cap the cloned
#     tree (CLONE_MAX_BYTES) so a hostile/huge repo cannot exhaust the staging
#     volume; both abort with exit 5 → the runtime's TooLarge outcome.
#
# NOT this script's responsibility (the M5 worker owns these): the CPU/PID/memory
# container limits, and the `agentforge.project_clone=<attempt_id>` container label
# used for reaping orphaned clone containers.
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

# --- SSRF defense-in-depth: refuse a host that resolves to a private range ----
# BEST-EFFORT only. The REAL control is the deploy-layer egress firewall on the
# clone egress network (it filters at packet time, defeating DNS-rebinding). This
# pre-resolve catches the common case (a URL pointing straight at 127.0.0.1 / a
# 10.x internal / 169.254.169.254 metadata) BEFORE git connects, failing closed
# with exit 2. It does NOT defend against a host that resolves to a public IP here
# but re-resolves to a private one at git's connect time (rebinding) — only the
# firewall does. We resolve via getent (glibc) and reject any matching A record.
is_blocked_ip() {
  # $1 = an IPv4/IPv6 address string. Returns 0 (true) if it is loopback,
  # private (RFC1918), link-local, CGNAT, metadata, or unspecified.
  case "$1" in
    127.*|0.*) return 0 ;;                          # loopback / "this host"
    10.*) return 0 ;;                               # RFC1918 /8
    192.168.*) return 0 ;;                          # RFC1918 /16
    169.254.*) return 0 ;;                          # link-local + 169.254.169.254 metadata
    100.6[4-9].*|100.7*.*|100.8*.*|100.9*.*|100.1[0-1].*|100.12[0-7].*) return 0 ;; # CGNAT 100.64/10
    255.255.255.255) return 0 ;;                    # broadcast
    ::1|::|fe80:*|fc00:*|fd*:*) return 0 ;;         # IPv6 loopback/unspecified/link-local/ULA
  esac
  # RFC1918 172.16.0.0/12 = 172.16.* .. 172.31.*
  case "$1" in
    172.1[6-9].*|172.2[0-9].*|172.3[0-1].*) return 0 ;;
  esac
  return 1
}

# `.local` is mDNS — never a legitimate public git host; refuse outright.
case "$TARGET_HOST" in
  *.local) fail 2 "CLONE_URL host '$TARGET_HOST' is a .local mDNS name — refusing (SSRF guard)" ;;
esac

# If the host is already a literal IP, check it directly; otherwise resolve every
# A/AAAA record and reject if ANY lands in a blocked range. A resolve failure is
# NOT fatal here (git will fail on its own with a clearer error, and the firewall
# is the real control) — we only act on a positive private-IP match.
resolved_addrs="$(getent ahosts "$TARGET_HOST" 2>/dev/null | awk '{print $1}' | sort -u)"
if [ -n "$resolved_addrs" ]; then
  while IFS= read -r addr; do
    [ -n "$addr" ] || continue
    if is_blocked_ip "$addr"; then
      fail 2 "CLONE_URL host '$TARGET_HOST' resolves to a blocked private/metadata address ($addr) — refusing (SSRF guard)"
    fi
  done <<EOF
$resolved_addrs
EOF
fi

# --- disk preflight: refuse if the staging filesystem is already too full -----
# A hostile/huge repo could otherwise fill the staging volume. `df -P -B1` reports
# available bytes in a portable single-column form. Floor defaults to CLONE_MAX_BYTES
# (we need at least room for the cap) or 64 MiB if no cap is set.
default_floor=67108864 # 64 MiB
CLONE_MIN_FREE_BYTES="${CLONE_MIN_FREE_BYTES:-${CLONE_MAX_BYTES:-$default_floor}}"
avail_bytes="$(df -P -B1 "$CLONE_DEST" 2>/dev/null | awk 'NR==2 {print $4}')"
case "$avail_bytes" in
  ''|*[!0-9]*) avail_bytes="" ;; # unparseable → skip the preflight (best-effort)
esac
if [ -n "$avail_bytes" ] && [ "$avail_bytes" -lt "$CLONE_MIN_FREE_BYTES" ]; then
  fail 5 "insufficient free space in $CLONE_DEST: ${avail_bytes}B available < ${CLONE_MIN_FREE_BYTES}B required"
fi

# --- assemble git -c credential args (host-scoped, one-shot helper) ----------
# GIT_CRED_ARGS is populated ONLY when a secret file is present. When the repo is
# public (no secret mounted) the helper is never configured and git clones
# anonymously.
GIT_CRED_ARGS=()
if [ -f "$SECRET_FILE" ]; then
  if [ ! -r "$SECRET_FILE" ]; then
    fail 3 "credential secret present at $SECRET_FILE but not readable (check mount mode 0644 + secret-root traversal + container uid)"
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

# --- disk watchdog (background) ----------------------------------------------
# A hostile/huge repo could exceed the size cap mid-clone (the preflight only
# checks free space at the START). This watchdog samples the cloned tree size and,
# if it crosses CLONE_MAX_BYTES, kills the clone and signals a too-large abort via
# a sentinel file. It exits cleanly (no abort) once the clone finishes. CLONE_MAX_BYTES
# empty/unset disables it. The watchdog runs in the SAME process group; we track
# its PID so we can stop it on every exit path.
WATCHDOG_PID=""
TOO_LARGE_SENTINEL="/tmp/agentforge-clone-too-large"
rm -f "$TOO_LARGE_SENTINEL"
start_disk_watchdog() {
  local clone_pid="$1"
  [ -n "${CLONE_MAX_BYTES:-}" ] || return 0
  case "$CLONE_MAX_BYTES" in ''|*[!0-9]*) return 0 ;; esac
  (
    while kill -0 "$clone_pid" 2>/dev/null; do
      sleep 2
      kill -0 "$clone_pid" 2>/dev/null || break
      used="$(du -sb "$REPO_DIR" 2>/dev/null | awk '{print $1}')"
      case "$used" in ''|*[!0-9]*) used=0 ;; esac
      if [ "$used" -gt "$CLONE_MAX_BYTES" ]; then
        log "ERROR: cloned tree ${used}B exceeded CLONE_MAX_BYTES=${CLONE_MAX_BYTES}B — aborting clone"
        : > "$TOO_LARGE_SENTINEL"
        # Kill the clone process group so git and any helpers stop promptly.
        kill -TERM "$clone_pid" 2>/dev/null
        sleep 2
        kill -KILL "$clone_pid" 2>/dev/null
        break
      fi
    done
  ) &
  WATCHDOG_PID="$!"
}
stop_disk_watchdog() {
  [ -n "$WATCHDOG_PID" ] || return 0
  kill "$WATCHDOG_PID" 2>/dev/null
  wait "$WATCHDOG_PID" 2>/dev/null
  WATCHDOG_PID=""
}
# Ensure the watchdog never outlives the script.
trap 'rm -f "$CRED_HELPER"; stop_disk_watchdog' EXIT

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
# host-scoped helper so the host scope is not also wiped. Run git in the
# background so the watchdog can monitor + kill it; then `wait` for its real rc.
GIT_LFS_SKIP_SMUDGE=1 GIT_TERMINAL_PROMPT=0 \
  git -c credential.helper= \
    "${GIT_CRED_ARGS[@]}" \
    clone --no-recurse-submodules "$CLONE_URL" "$REPO_DIR" &
clone_pid="$!"
start_disk_watchdog "$clone_pid"
if wait "$clone_pid"; then
  clone_rc=0
else
  clone_rc=$?
fi
stop_disk_watchdog

# If the watchdog tripped, the abort takes precedence over git's own (kill-induced)
# non-zero exit — surface the distinct exit 5 so the runtime maps it to TooLarge.
if [ -f "$TOO_LARGE_SENTINEL" ]; then
  rm -f "$TOO_LARGE_SENTINEL"
  fail 5 "clone aborted: cloned tree exceeded CLONE_MAX_BYTES=${CLONE_MAX_BYTES}B"
fi
if [ "$clone_rc" -eq 0 ]; then
  log "clone succeeded"
else
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

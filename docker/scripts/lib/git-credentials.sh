# shellcheck shell=bash
# =============================================================================
# Wisdoverse Forge — shared git credential / host / hardening library
# =============================================================================
# Sourceable shell functions factored out of agent-entrypoint.sh so the same
# git-platform setup can be reused by the ephemeral agentforge-clone container.
#
# IMPORTANT: callers (agent-entrypoint.sh) run WITHOUT `set -e`. These functions
# must therefore be robust on their own — log every failure explicitly, never
# rely on `set -e` to abort, and never let a single failed step silently skip
# the rest. Each function is best-effort and returns 0 unless documented.
#
# SECURITY: no function in this library ever echoes a credential value. Tokens
# are written into config files (chmod 600) or consumed by git directly; they
# are never printed, logged, or placed on a command line that ends up in
# process listings.
#
# These functions intentionally read their inputs from the same environment
# variables the inline entrypoint code used, so behavior is preserved exactly:
#   GITLAB_TOKEN, GITLAB_HOST           -> configure_git_credentials
#   SELF_HOSTED_GITLAB_SSH              -> configure_known_hosts (glab SSH rewrites)
#   AGENTFORGE_CUSTOM_GIT_HOSTS         -> configure_custom_git_hosts
#   AGENTFORGE_GIT_LFS_SKIP             -> configure_git_hardening
#   HOME                                -> path roots
# =============================================================================

# Log helper — keeps the "agent-entrypoint:" prefix so existing log scrapers and
# operator expectations are unchanged when sourced from the agent entrypoint.
# A caller may override AGENTFORGE_LOG_PREFIX (the clone entrypoint uses its own).
_gitlib_log() {
  printf '%s %s\n' "${AGENTFORGE_LOG_PREFIX:-agent-entrypoint:}" "$*"
}

# -----------------------------------------------------------------------------
# configure_git_credentials
# -----------------------------------------------------------------------------
# Materialize git-platform CLI credentials from server-injected env tokens.
#
#   - GitHub: GH_TOKEN / GITHUB_TOKEN (and the *_ENTERPRISE_TOKEN variants) are
#     read directly by the `gh` CLI, so they are intentionally LEFT in the
#     environment — there is nothing to write here. (Documented for clarity.)
#   - GitLab: GITLAB_TOKEN / GITLAB_HOST are converted into the config file that
#     `glab` expects, then the raw env tokens are cleared so they cannot leak via
#     `printenv` / /proc/<pid>/environ to the agent CLI process.
#
# Behavior-preserving copy of the former inline block. Never prints the token.
configure_git_credentials() {
  # glab CLI: requires ~/.config/glab-cli/config.yml
  if [ -n "${GITLAB_TOKEN:-}" ]; then
    local glab_config_dir glab_host
    glab_config_dir="${HOME:-/home/agent}/.config/glab-cli"
    glab_host="${GITLAB_HOST:-gitlab.com}"

    if mkdir -p "$glab_config_dir"; then
      cat > "$glab_config_dir/config.yml" <<GLAB_EOF
hosts:
  ${glab_host}:
    token: ${GITLAB_TOKEN}
    api_host: ${glab_host}
    git_protocol: ssh
GLAB_EOF
      chmod 600 "$glab_config_dir/config.yml"
      _gitlib_log "Configured glab CLI for host: $glab_host"
    else
      _gitlib_log "WARNING: Failed to create $glab_config_dir — glab may prompt for auth"
    fi

    # Clear token from env to prevent leakage via printenv / /proc/*/environ
    unset GITLAB_TOKEN
    unset GITLAB_HOST
  fi
}

# -----------------------------------------------------------------------------
# configure_custom_git_hosts <known_hosts_file>
# -----------------------------------------------------------------------------
# Scan AGENTFORGE_CUSTOM_GIT_HOSTS (comma-separated) and append ed25519/ecdsa
# host keys for each validated hostname into the given known_hosts file.
# Hostnames are validated against an injection-safe pattern before use.
#
# Behavior-preserving copy of the former inline custom-host scan. No secrets.
configure_custom_git_hosts() {
  local known_hosts_file="$1"
  [ -n "${AGENTFORGE_CUSTOM_GIT_HOSTS:-}" ] || return 0
  [ -n "$known_hosts_file" ] || return 0

  local custom_hosts host
  IFS=',' read -ra custom_hosts <<< "$AGENTFORGE_CUSTOM_GIT_HOSTS"
  for host in "${custom_hosts[@]}"; do
    host=$(echo "$host" | xargs)  # trim whitespace
    # Validate hostname: alphanumeric, dots, hyphens only (prevent command injection)
    if [ -z "$host" ] || ! echo "$host" | grep -qE '^[a-zA-Z0-9][a-zA-Z0-9._-]+$'; then
      _gitlib_log "WARNING: Skipping invalid custom git host: '$host'"
      continue
    fi
    if ! grep -qF "$host " "$known_hosts_file" 2>/dev/null; then
      if ssh-keyscan -t ed25519,ecdsa "$host" >> "$known_hosts_file" 2>&1; then
        _gitlib_log "Added host keys for custom git host: $host"
      else
        _gitlib_log "WARNING: Failed to scan host keys for: $host"
      fi
    fi
  done
}

# -----------------------------------------------------------------------------
# configure_known_hosts
# -----------------------------------------------------------------------------
# Set up SSH keys for git access from the read-only host mount at /host-ssh-keys,
# configure git/glab to prefer SSH for GitLab providers, apply self-hosted GitLab
# SSH rewrites, and scan known_hosts for any custom git hosts. If the mount is
# absent this is a no-op (logged).
#
# Behavior-preserving copy of the former inline "Set up SSH keys" block. The
# function calls configure_custom_git_hosts() to scan custom hosts.
configure_known_hosts() {
  local ssh_mount="${1:-/host-ssh-keys}"
  if [ ! -d "$ssh_mount" ]; then
    _gitlib_log "No SSH key mount found at $ssh_mount"
    return 0
  fi

  _gitlib_log "Found SSH keys mount at $ssh_mount"

  local ssh_dir
  ssh_dir="${HOME:-/home/agent}/.ssh"
  if ! { mkdir -p "$ssh_dir" && chmod 700 "$ssh_dir"; }; then
    _gitlib_log "ERROR: Failed to create $ssh_dir — skipping SSH key setup"
    return 0
  fi

  local copied=0 f basename_f
  for f in "$ssh_mount"/id_*; do
    [ -f "$f" ] || continue
    basename_f="$(basename "$f")"
    if cp "$f" "$ssh_dir/$basename_f"; then
      # Private keys get 600, public keys and config get 644
      case "$basename_f" in
        *.pub) chmod 644 "$ssh_dir/$basename_f" ;;
        *)     chmod 600 "$ssh_dir/$basename_f" ;;
      esac
      copied=$((copied + 1))
    else
      _gitlib_log "WARNING: Failed to copy $basename_f to $ssh_dir"
    fi
  done

  # Copy config and known_hosts
  for f in config known_hosts; do
    if [ -f "$ssh_mount/$f" ]; then
      if cp "$ssh_mount/$f" "$ssh_dir/$f"; then
        chmod 644 "$ssh_dir/$f"
      else
        _gitlib_log "WARNING: Failed to copy $f to $ssh_dir"
      fi
    fi
  done

  if [ "$copied" -gt 0 ]; then
    _gitlib_log "Copied $copied SSH key file(s) to $ssh_dir"
    # Configure git to use SSH for common providers
    git config --global core.sshCommand "ssh -F $ssh_dir/config" 2>/dev/null || true

    # Configure glab CLI to prefer SSH protocol for GitLab operations
    if command -v glab &> /dev/null; then
      if git config --global url."git@gitlab.com:".insteadOf "https://gitlab.com/" 2>/dev/null; then
        _gitlib_log "Configured git to use SSH for GitLab (glab CLI)"
      fi
      # Configure additional self-hosted GitLab SSH rewrites via SELF_HOSTED_GITLAB_SSH
      # Format: "ssh.gitlab.example.com=https://gitlab.example.com/" (comma-separated for multiple)
      if [ -n "${SELF_HOSTED_GITLAB_SSH:-}" ]; then
        local rewrite_count=0 rewrites rewrite ssh_host https_url
        IFS=',' read -ra rewrites <<< "$SELF_HOSTED_GITLAB_SSH"
        for rewrite in "${rewrites[@]}"; do
          # Validate format: must contain exactly one '='
          if [[ "$rewrite" != *"="* ]]; then
            _gitlib_log "WARNING: Skipping malformed SELF_HOSTED_GITLAB_SSH entry (missing '='): $rewrite"
            continue
          fi
          ssh_host="${rewrite%%=*}"
          https_url="${rewrite##*=}"
          if [ -z "$ssh_host" ] || [ -z "$https_url" ]; then
            _gitlib_log "WARNING: Skipping incomplete SELF_HOSTED_GITLAB_SSH entry: $rewrite"
            continue
          fi
          if git config --global url."git@${ssh_host}:".insteadOf "$https_url" 2>/dev/null; then
            _gitlib_log "Configured SSH rewrite for $ssh_host → $https_url"
            rewrite_count=$((rewrite_count + 1))
          else
            _gitlib_log "WARNING: Failed to configure SSH rewrite for $ssh_host"
          fi
        done
        _gitlib_log "Configured $rewrite_count SSH rewrite(s) total"
      fi
    fi

    # Note: we do NOT set url."git@github.com:".insteadOf for GitHub.
    # Users' git clone URLs should be used as-is — HTTPS stays HTTPS, SSH stays SSH.
    # The gh CLI handles its own auth via tokens and doesn't need insteadOf.

    # Scan custom git hosts for known_hosts (AGENTFORGE_CUSTOM_GIT_HOSTS=host1,host2)
    configure_custom_git_hosts "$ssh_dir/known_hosts"
  else
    _gitlib_log "WARNING: SSH mount found but no key files copied"
  fi
}

# -----------------------------------------------------------------------------
# configure_git_hardening
# -----------------------------------------------------------------------------
# Apply global git hardening (diff/rename/bigfile limits, core.fileMode false for
# Docker volume mounts) and conditionally disable git-lfs smudge/process to avoid
# runaway I/O. Controlled by AGENTFORGE_GIT_LFS_SKIP (default: skip).
#
# Behavior-preserving copy of the former inline "Git hardening" block.
configure_git_hardening() {
  # Limit diff output to prevent memory/PID exhaustion from large repos
  git config --global diff.renameLimit 200
  git config --global core.bigFileThreshold 5m
  # Docker volume mounts lose POSIX permission bits (everything becomes 755).
  # Without this, git sees every file as modified, spawning hundreds of
  # git+git-lfs processes that exhaust the container's memory/PID limits.
  git config --global core.fileMode false

  # Conditionally disable git-lfs filter to prevent runaway I/O.
  # LFS pointers remain as-is; agents work with source code, not large binaries.
  # Controlled by resource profile: AGENTFORGE_GIT_LFS_SKIP=true (default) or false.
  # Unset defaults to skip (the historical behavior); `:-` keeps this robust even
  # under a `set -u` caller, matching the empty-string -> skip path.
  if [ "${AGENTFORGE_GIT_LFS_SKIP:-}" != "false" ]; then
    git config --global filter.lfs.smudge "git-lfs smudge --skip -- %f"
    git config --global filter.lfs.process "git-lfs filter-process --skip"
    git config --global filter.lfs.required false
    _gitlib_log "git-lfs disabled (skip mode)"
  else
    _gitlib_log "git-lfs enabled"
  fi
}

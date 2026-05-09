#!/bin/bash
# NOTE: No `set -e` — setup failures should be logged but not prevent the CLI from starting.
# Process model: tini (PID 1) -> entrypoint.sh -> script (PTY wrapper) -> CLI process.
# tini forwards SIGTERM to the entire process group (-g) on `docker stop`.
# When a sidecar runs, bash stays alive to clean it up after CLI exits.
# Without a sidecar, `exec` replaces bash with script/CLI directly.

# Legacy HTTP event endpoint removed — events are now delivered via NATS sidecar.
# AGENTFORGE_SERVER_URL is still used for other server communication.

# =============================================================================
# Resolve CLI tool configuration
# =============================================================================
# AGENTFORGE_CLI_TOOL is set by:
#   1. Docker image ENV (baked in at build time via CLI_TOOL build arg)
#   2. Container runtime env override (from SessionService)
# Container env takes precedence over image ENV.

CLI_TOOL="${AGENTFORGE_CLI_TOOL:-claude}"
HOOKS_FILE=""

case "$CLI_TOOL" in
  claude)
    CREDS_DIR=~/.claude
    SETTINGS_FILE=~/.claude/settings.json
    DEFAULT_CMD="claude --dangerously-skip-permissions"
    HOOK_COMPAT="native"
    CLI_MODE="line"         # line-mode CLI — works with any terminal size
    ;;
  gemini)
    CREDS_DIR=~/.gemini
    SETTINGS_FILE=~/.gemini/settings.json
    DEFAULT_CMD="gemini --yolo --skip-trust"
    HOOK_COMPAT="native"
    CLI_MODE="line"
    # Gemini CLI relaunches itself as a child process for memory tuning.
    # The parent pauses stdin before spawning the child, which breaks stdin
    # delivery through the Docker PTY → script PTY chain.
    export GEMINI_CLI_NO_RELAUNCH=true
    export GEMINI_CLI_TRUST_WORKSPACE=true
    ;;
  opencode)
    CREDS_DIR=~/.local/share/opencode
    SETTINGS_FILE=~/.config/opencode/opencode.json
    DEFAULT_CMD="opencode"
    HOOK_COMPAT="notify"
    CLI_MODE="tui"          # full-screen TUI — requires non-zero terminal size
    ;;
  codex)
    CREDS_DIR=~/.codex
    SETTINGS_FILE=~/.codex/config.toml
    HOOKS_FILE=~/.codex/hooks.json
    # Prefer the compact YOLO permission alias when available. Codex CLI 0.125.0
    # still documents only the long dangerous flag, so keep a runtime fallback.
    DEFAULT_CMD="codex --yolo"
    DEFAULT_CMD_FALLBACK="codex --dangerously-bypass-approvals-and-sandbox"
    HOOK_COMPAT="native"
    CLI_MODE="tui"          # Ratatui-based TUI — requires non-zero terminal size
    ;;
  *)
    echo "agent-entrypoint: FATAL: Unknown CLI_TOOL '$CLI_TOOL'. Supported: claude, opencode, codex, gemini"
    echo "agent-entrypoint: Update the entrypoint script if a new tool was added."
    exit 1
    ;;
esac

# If CLAUDE_CONFIG_DIR is set (normally stripped by server env filtering),
# respect it so the entrypoint stays robust when invoked outside the normal server flow.
if [ -n "$CLAUDE_CONFIG_DIR" ]; then
  if [ "$CLI_TOOL" = "claude" ]; then
    echo "agent-entrypoint: CLAUDE_CONFIG_DIR is set ($CLAUDE_CONFIG_DIR), overriding CREDS_DIR"
    CREDS_DIR="$CLAUDE_CONFIG_DIR"
    SETTINGS_FILE="$CLAUDE_CONFIG_DIR/settings.json"
  else
    echo "agent-entrypoint: WARNING: CLAUDE_CONFIG_DIR is set but CLI_TOOL=$CLI_TOOL — ignoring"
  fi
fi

echo "agent-entrypoint: CLI tool = $CLI_TOOL (mode: $CLI_MODE, hooks: $HOOK_COMPAT)"
echo "agent-entrypoint: Credentials dir = $CREDS_DIR"

# =============================================================================
# Wait for session allocation (pool containers only)
# =============================================================================
# Pool-managed containers start without session context. The Go platform writes
# /tmp/session.env via injectSessionEnv() when a session is allocated.
# We MUST wait for it before proceeding — the env contains auth keys, session ID,
# server URL, and other critical config needed by the CLI.

if [ "$AGENTFORGE_POOL_MANAGED" = "true" ]; then
  # Graceful shutdown: exit cleanly when pool manager terminates this container.
  # Without the trap, SIGTERM terminates bash with exit code 143 (128+SIGTERM).
  POOL_TERMINATED=false
  trap 'POOL_TERMINATED=true' TERM INT

  echo "agent-entrypoint: Pool-managed container — waiting for session allocation..."

  WAIT_SECONDS=0
  # Safety TTL: if pool manager can't evict (crash/partition), self-terminate.
  # The pool manager injects the actual value (2× its configured TTL) via env var.
  # 7200s is a last-resort fallback when the env var is absent.
  IDLE_TTL="${AGENTFORGE_POOL_IDLE_TTL:-7200}"

  while [ ! -f /tmp/session.env ] && [ "$POOL_TERMINATED" = "false" ]; do
    sleep 1 &
    wait $! 2>/dev/null  # wait returns immediately on trapped signals; foreground sleep blocks until duration elapses
    WAIT_SECONDS=$((WAIT_SECONDS + 1))

    if [ $((WAIT_SECONDS % 300)) -eq 0 ]; then
      echo "agent-entrypoint: Still waiting for session allocation (${WAIT_SECONDS}s / ${IDLE_TTL}s TTL)..."
    fi

    # Safety timeout — should never fire if pool manager is healthy
    if [ "$IDLE_TTL" -gt 0 ] && [ "$WAIT_SECONDS" -ge "$IDLE_TTL" ]; then
      echo "agent-entrypoint: Safety TTL expired (${IDLE_TTL}s) — pool manager may be unavailable"
      exit 1
    fi
  done

  # Clean exit on SIGTERM (pool manager terminated us)
  if [ "$POOL_TERMINATED" = "true" ]; then
    echo "agent-entrypoint: Received termination signal — exiting cleanly"
    exit 0
  fi

  # Reset trap before proceeding with session work
  trap - TERM INT

  # shellcheck disable=SC1091
  . /tmp/session.env
  echo "agent-entrypoint: Session allocated — sourced /tmp/session.env (waited ${WAIT_SECONDS}s)"
fi

# =============================================================================
# Inject credentials from server-provided sources
# =============================================================================
# Credentials are injected per-session by the server via one of:
#   - API key env var (highest priority, does not expire)
#   - OAuth file mount at /run/secrets/oauth-credentials/ (from DB)
#   - OAuth env var AGENTFORGE_OAUTH_CREDENTIALS (legacy fallback)
# The deprecated host mount (/host-claude-credentials) is no longer used.

# Determine the correct API key env var for this CLI tool's provider.
# If an API key is set, skip OAuth credential copy entirely.
# API keys don't expire and take priority over OAuth tokens.
case "$CLI_TOOL" in
  claude|opencode)
    AUTH_ENV_VAR="ANTHROPIC_API_KEY"
    ;;
  gemini)
    AUTH_ENV_VAR="GEMINI_API_KEY"
    ;;
  codex)
    AUTH_ENV_VAR="OPENAI_API_KEY"
    ;;
  *)
    AUTH_ENV_VAR=""
    echo "agent-entrypoint: WARNING: No API key env var mapped for CLI_TOOL=$CLI_TOOL"
    ;;
esac

if [ -n "${!AUTH_ENV_VAR:-}" ]; then
  echo "agent-entrypoint: $AUTH_ENV_VAR is set — using API key auth (skipping OAuth credential copy)"
  SKIP_CREDS=true
elif [ -f /run/secrets/oauth-credentials/credentials ]; then
  # File-mounted OAuth credentials (cloud-native pattern).
  # The server writes the credential blob to a host-side temp dir and bind-mounts
  # it at /run/secrets/oauth-credentials/ (read-only), avoiding the practical Docker
  # environment size limit that large env vars can exceed.
  echo "agent-entrypoint: Found file-mounted OAuth credentials — injecting"
  mkdir -p "$CREDS_DIR"
  if CREDS_DIR="$CREDS_DIR" node -e "
    const fs = require('fs');
    const path = require('path');
    const blob = fs.readFileSync('/run/secrets/oauth-credentials/credentials', 'utf8');
    const files = JSON.parse(Buffer.from(blob, 'base64').toString());
    const dir = process.env.CREDS_DIR;
    for (const [name, content] of Object.entries(files)) {
      const safeName = path.basename(name);
      const filePath = path.join(dir, safeName);
      fs.writeFileSync(filePath, content, 'utf8');
      console.log('agent-entrypoint: Wrote ' + name + ' to ' + dir);
    }
  " 2>&1; then
    echo "agent-entrypoint: File-mounted OAuth credentials injected successfully"
  else
    echo "agent-entrypoint: ERROR: Failed to decode/write OAuth credentials from file mount"
    echo "agent-entrypoint: ERROR: The CLI will likely fail to authenticate — check mount contents"
  fi
  SKIP_CREDS=true
elif [ -n "${AGENTFORGE_OAUTH_CREDENTIALS:-}" ]; then
  # Legacy: env var fallback for backward compatibility with small credential blobs.
  echo "agent-entrypoint: Found DB-backed OAuth credentials (env var) — injecting"
  mkdir -p "$CREDS_DIR"
  if CREDS_DIR="$CREDS_DIR" node -e "
    const fs = require('fs');
    const path = require('path');
    const files = JSON.parse(Buffer.from(process.env.AGENTFORGE_OAUTH_CREDENTIALS, 'base64').toString());
    const dir = process.env.CREDS_DIR;
    for (const [name, content] of Object.entries(files)) {
      const safeName = path.basename(name);
      const filePath = path.join(dir, safeName);
      fs.writeFileSync(filePath, content, 'utf8');
      console.log('agent-entrypoint: Wrote ' + name + ' to ' + dir);
    }
  " 2>&1; then
    echo "agent-entrypoint: OAuth credentials injected successfully"
  else
    echo "agent-entrypoint: ERROR: Failed to decode OAuth credentials from env var"
  fi
  unset AGENTFORGE_OAUTH_CREDENTIALS
  SKIP_CREDS=true
else
  SKIP_CREDS=false
fi

if [ "$SKIP_CREDS" = "false" ] && ! mkdir -p "$CREDS_DIR"; then
  echo "agent-entrypoint: ERROR: Failed to create $CREDS_DIR — skipping credential setup"
  SKIP_CREDS=true
fi

# Check if credential directory already has auth files (from a persistent volume or prior run)
if [ "$SKIP_CREDS" = "false" ]; then
  EXISTING_CREDS=$(find "$CREDS_DIR" -maxdepth 1 \( -name '*.json' -o -name '.*.json' \) 2>/dev/null | head -1)
  if [ -n "$EXISTING_CREDS" ]; then
    echo "agent-entrypoint: Found existing credentials in $CREDS_DIR — skipping credential copy"
    SKIP_CREDS=true
  fi
fi

# Host credential mount (/host-claude-credentials) is no longer supported.
# Credentials are injected per-session by the server via:
#   1. API key env var (ANTHROPIC_API_KEY / GEMINI_API_KEY / OPENAI_API_KEY)
#   2. OAuth file mount at /run/secrets/oauth-credentials/
#   3. OAuth env var AGENTFORGE_OAUTH_CREDENTIALS (legacy fallback)
# The shared host mount was removed because it leaked credentials across CLI
# tools (e.g. Claude OAuth tokens copied into Gemini containers).
if [ -d "/host-claude-credentials" ]; then
  echo "agent-entrypoint: WARNING: /host-claude-credentials mount detected but ignored (deprecated)"
  echo "agent-entrypoint: WARNING: Configure per-user API keys or use CONTAINER_*_API_KEY system fallback instead"
fi

if [ "$SKIP_CREDS" = "false" ]; then
  echo "agent-entrypoint: No server-injected credentials found for $CLI_TOOL"
  echo "agent-entrypoint: $CLI_TOOL may require manual authentication or a system fallback API key"
fi

# Verify credentials are available (skip when using API key)
if [ -n "${!AUTH_ENV_VAR:-}" ]; then
  echo "agent-entrypoint: Auth method: API key ($AUTH_ENV_VAR)"
  if [ -n "${AGENTFORGE_CREDENTIAL_SOURCE:-}" ]; then
    echo "agent-entrypoint: Credential source: $AGENTFORGE_CREDENTIAL_SOURCE"
  fi
else
  CRED_FILES=$(find "$CREDS_DIR" -maxdepth 1 \( -name '*.json' -o -name '.*.json' \) 2>/dev/null | head -5)
  if [ -n "$CRED_FILES" ]; then
    echo "agent-entrypoint: Auth method: OAuth credentials"
    echo "$CRED_FILES" | while read -r f; do echo "  - $(basename "$f")"; done
  else
    echo "agent-entrypoint: WARNING: No credential files found in $CREDS_DIR"
    echo "agent-entrypoint: WARNING: $CLI_TOOL will likely fail to authenticate"
  fi
fi

# =============================================================================
# Git CLI credential injection (glab, gh)
# =============================================================================
# The server injects Git platform tokens from the Rust git_credentials service:
#   - GitHub: GH_TOKEN/GITHUB_TOKEN or GH_ENTERPRISE_TOKEN/GITHUB_ENTERPRISE_TOKEN.
#     gh reads these directly, so we leave them in env for the CLI process.
#   - GitLab: GITLAB_TOKEN/GITLAB_HOST. Convert these into the config file that
#     glab expects, then clear the raw env vars below.

# glab CLI: requires ~/.config/glab-cli/config.yml
if [ -n "${GITLAB_TOKEN:-}" ]; then
  GLAB_CONFIG_DIR="/home/agent/.config/glab-cli"
  GLAB_HOST="${GITLAB_HOST:-gitlab.com}"

  if mkdir -p "$GLAB_CONFIG_DIR"; then
    cat > "$GLAB_CONFIG_DIR/config.yml" <<GLAB_EOF
hosts:
  ${GLAB_HOST}:
    token: ${GITLAB_TOKEN}
    api_host: ${GLAB_HOST}
    git_protocol: ssh
GLAB_EOF
    chmod 600 "$GLAB_CONFIG_DIR/config.yml"
    echo "agent-entrypoint: Configured glab CLI for host: $GLAB_HOST"
  else
    echo "agent-entrypoint: WARNING: Failed to create $GLAB_CONFIG_DIR — glab may prompt for auth"
  fi

  # Clear token from env to prevent leakage via printenv / /proc/*/environ
  unset GITLAB_TOKEN
  unset GITLAB_HOST
fi

# =============================================================================
# Configure Wisdoverse Forge hooks in CLI settings
# =============================================================================
# Only write hooks for tools with 'native' or 'adapter' hook compatibility.
# Tools with 'notify' compatibility don't support hook registration.

if [ "$HOOK_COMPAT" = "native" ] || [ "$HOOK_COMPAT" = "adapter" ]; then
  HOOK_CMD="node $HOME/.agentforge/hooks/agentforge-relay-hook.cjs"
  TEMPLATE="$HOME/.agentforge/hooks/templates/${CLI_TOOL}.json"
  HOOK_TARGET="${HOOKS_FILE:-$SETTINGS_FILE}"

  if [ -f "$TEMPLATE" ]; then
    mkdir -p "$(dirname "$HOOK_TARGET")"
    sed "s|__HOOK_CMD__|${HOOK_CMD}|g" "$TEMPLATE" > "$HOOK_TARGET"
    echo "agent-entrypoint: Hooks configured from template ($CLI_TOOL) in $HOOK_TARGET"
  else
    echo "agent-entrypoint: WARNING: No hook template for $CLI_TOOL at $TEMPLATE — hooks will not be registered"
  fi

  # Gemini CLI: inject auth type into settings to skip the interactive auth
  # selection dialog. The auth type is determined by which credential source
  # is available (API key vs OAuth).
  if [ "$CLI_TOOL" = "gemini" ] && [ -f "$SETTINGS_FILE" ]; then
    if [ -n "${GEMINI_API_KEY:-}" ]; then
      GEMINI_AUTH_TYPE="gemini-api-key"
    else
      GEMINI_AUTH_TYPE="oauth-personal"
    fi
    node -e "
      const fs = require('fs');
      const s = JSON.parse(fs.readFileSync('$SETTINGS_FILE', 'utf8'));
      s.security = { auth: { selectedType: '$GEMINI_AUTH_TYPE' } };
      fs.writeFileSync('$SETTINGS_FILE', JSON.stringify(s, null, 2));
    " 2>/dev/null && echo "agent-entrypoint: Gemini auth type set to $GEMINI_AUTH_TYPE"
  fi
else
  echo "agent-entrypoint: Skipping hook registration (hook compatibility: $HOOK_COMPAT)"
fi

# =============================================================================
# Source pool-injected session environment (if present)
# =============================================================================
# When containers are acquired from the pool, session-specific env vars
# (AGENTFORGE_SERVER_URL, AGENTFORGE_SESSION_ID, etc.) are written to
# /tmp/session.env by ContainerRuntime.injectSessionEnv().

if [ -f /tmp/session.env ]; then
  # shellcheck disable=SC1091
  . /tmp/session.env
  echo "agent-entrypoint: Sourced session environment from /tmp/session.env"
fi

# =============================================================================
# Set up SSH keys for git access (if mounted)
# =============================================================================
# The host's SSH keys directory is mounted at /host-ssh-keys (read-only).
# We copy keys to ~/.ssh/ so git can authenticate with private repositories.

SSH_MOUNT="/host-ssh-keys"
if [ -d "$SSH_MOUNT" ]; then
  echo "agent-entrypoint: Found SSH keys mount at $SSH_MOUNT"

  SSH_DIR="$HOME/.ssh"
  if mkdir -p "$SSH_DIR" && chmod 700 "$SSH_DIR"; then
    copied=0
    for f in "$SSH_MOUNT"/id_*; do
      [ -f "$f" ] || continue
      basename_f="$(basename "$f")"
      if cp "$f" "$SSH_DIR/$basename_f"; then
        # Private keys get 600, public keys and config get 644
        case "$basename_f" in
          *.pub) chmod 644 "$SSH_DIR/$basename_f" ;;
          *)     chmod 600 "$SSH_DIR/$basename_f" ;;
        esac
        copied=$((copied + 1))
      else
        echo "agent-entrypoint: WARNING: Failed to copy $basename_f to $SSH_DIR"
      fi
    done

    # Copy config and known_hosts
    for f in config known_hosts; do
      if [ -f "$SSH_MOUNT/$f" ]; then
        if cp "$SSH_MOUNT/$f" "$SSH_DIR/$f"; then
          chmod 644 "$SSH_DIR/$f"
        else
          echo "agent-entrypoint: WARNING: Failed to copy $f to $SSH_DIR"
        fi
      fi
    done

    if [ "$copied" -gt 0 ]; then
      echo "agent-entrypoint: Copied $copied SSH key file(s) to $SSH_DIR"
      # Configure git to use SSH for common providers
      git config --global core.sshCommand "ssh -F $SSH_DIR/config" 2>/dev/null || true

      # Configure glab CLI to prefer SSH protocol for GitLab operations
      if command -v glab &> /dev/null; then
        if git config --global url."git@gitlab.com:".insteadOf "https://gitlab.com/" 2>/dev/null; then
          echo "agent-entrypoint: Configured git to use SSH for GitLab (glab CLI)"
        fi
        # Configure additional self-hosted GitLab SSH rewrites via SELF_HOSTED_GITLAB_SSH
        # Format: "ssh.gitlab.example.com=https://gitlab.example.com/" (comma-separated for multiple)
        if [ -n "${SELF_HOSTED_GITLAB_SSH:-}" ]; then
          rewrite_count=0
          IFS=',' read -ra REWRITES <<< "$SELF_HOSTED_GITLAB_SSH"
          for rewrite in "${REWRITES[@]}"; do
            # Validate format: must contain exactly one '='
            if [[ "$rewrite" != *"="* ]]; then
              echo "agent-entrypoint: WARNING: Skipping malformed SELF_HOSTED_GITLAB_SSH entry (missing '='): $rewrite"
              continue
            fi
            ssh_host="${rewrite%%=*}"
            https_url="${rewrite##*=}"
            if [ -z "$ssh_host" ] || [ -z "$https_url" ]; then
              echo "agent-entrypoint: WARNING: Skipping incomplete SELF_HOSTED_GITLAB_SSH entry: $rewrite"
              continue
            fi
            if git config --global url."git@${ssh_host}:".insteadOf "$https_url" 2>/dev/null; then
              echo "agent-entrypoint: Configured SSH rewrite for $ssh_host → $https_url"
              rewrite_count=$((rewrite_count + 1))
            else
              echo "agent-entrypoint: WARNING: Failed to configure SSH rewrite for $ssh_host"
            fi
          done
          echo "agent-entrypoint: Configured $rewrite_count SSH rewrite(s) total"
        fi
      fi

      # Note: we do NOT set url."git@github.com:".insteadOf for GitHub.
      # Users' git clone URLs should be used as-is — HTTPS stays HTTPS, SSH stays SSH.
      # The gh CLI handles its own auth via tokens and doesn't need insteadOf.

      # Scan custom git hosts for known_hosts (AGENTFORGE_CUSTOM_GIT_HOSTS=host1,host2)
      if [ -n "${AGENTFORGE_CUSTOM_GIT_HOSTS:-}" ]; then
        IFS=',' read -ra CUSTOM_HOSTS <<< "$AGENTFORGE_CUSTOM_GIT_HOSTS"
        for host in "${CUSTOM_HOSTS[@]}"; do
          host=$(echo "$host" | xargs)  # trim whitespace
          # Validate hostname: alphanumeric, dots, hyphens only (prevent command injection)
          if [ -z "$host" ] || ! echo "$host" | grep -qE '^[a-zA-Z0-9][a-zA-Z0-9._-]+$'; then
            echo "agent-entrypoint: WARNING: Skipping invalid custom git host: '$host'"
            continue
          fi
          if ! grep -qF "$host " "$SSH_DIR/known_hosts" 2>/dev/null; then
            if ssh-keyscan -t ed25519,ecdsa "$host" >> "$SSH_DIR/known_hosts" 2>&1; then
              echo "agent-entrypoint: Added host keys for custom git host: $host"
            else
              echo "agent-entrypoint: WARNING: Failed to scan host keys for: $host"
            fi
          fi
        done
      fi
    else
      echo "agent-entrypoint: WARNING: SSH mount found but no key files copied"
    fi
  else
    echo "agent-entrypoint: ERROR: Failed to create $SSH_DIR — skipping SSH key setup"
  fi
else
  echo "agent-entrypoint: No SSH key mount found at $SSH_MOUNT"
fi

# =============================================================================
# Git hardening
# =============================================================================
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
if [ "${AGENTFORGE_GIT_LFS_SKIP}" != "false" ]; then
    git config --global filter.lfs.smudge "git-lfs smudge --skip -- %f"
    git config --global filter.lfs.process "git-lfs filter-process --skip"
    git config --global filter.lfs.required false
    echo "agent-entrypoint: git-lfs disabled (skip mode)"
else
    echo "agent-entrypoint: git-lfs enabled"
fi

# =========================================================================
# Skills setup: merge global + project skills into CLI tool's skills dir
# =========================================================================

SKILLS_GLOBAL="/home/agent/.agentforge/skills/global"
SKILLS_PROJECT="/home/agent/.agentforge/skills/project"

# Determine target skills directory based on CLI tool
case "$AGENTFORGE_CLI_TOOL" in
  claude)   SKILLS_TARGET="/home/agent/.claude/skills" ;;
  opencode) SKILLS_TARGET="/home/agent/.config/opencode/skills" ;;
  codex)    SKILLS_TARGET="/home/agent/.codex/skills" ;;
  gemini)   SKILLS_TARGET="/home/agent/.gemini/skills" ;;
  *)        SKILLS_TARGET="/home/agent/.claude/skills" ;;
esac

mkdir -p "$SKILLS_TARGET"

# Symlink global skills first
if [ -d "$SKILLS_GLOBAL" ]; then
  for skill_dir in "$SKILLS_GLOBAL"/*/; do
    [ -d "$skill_dir" ] || continue
    skill_name=$(basename "$skill_dir")
    ln -sfn "$skill_dir" "$SKILLS_TARGET/$skill_name"
  done
fi

# Symlink project skills (overrides global on name conflict)
if [ -d "$SKILLS_PROJECT" ]; then
  for skill_dir in "$SKILLS_PROJECT"/*/; do
    [ -d "$skill_dir" ] || continue
    skill_name=$(basename "$skill_dir")
    ln -sfn "$skill_dir" "$SKILLS_TARGET/$skill_name"
  done
fi

# =============================================================================
# Agent harness: inject CLAUDE.md + slash commands into workspace
# =============================================================================
# The harness provides behavioral guidelines and reusable commands for agents.
# CLAUDE.md is copied to the workspace root (if no project CLAUDE.md exists).
# Commands are symlinked into the CLI's commands directory.

HARNESS_DIR="/home/agent/.agentforge/harness"

if [ "$CLI_TOOL" = "claude" ]; then
  # Inject agent CLAUDE.md as a global-level CLAUDE.md (lowest precedence).
  # Project-level CLAUDE.md in /workspace takes priority automatically.
  # Always overwrite — the image-baked version is the source of truth.
  # On reused home volumes the previous copy would otherwise become stale.
  GLOBAL_CLAUDE_MD="$HOME/.claude/CLAUDE.md"
  if [ -f "$HARNESS_DIR/CLAUDE.md" ]; then
    cp "$HARNESS_DIR/CLAUDE.md" "$GLOBAL_CLAUDE_MD"
    echo "agent-entrypoint: Injected agent harness CLAUDE.md → $GLOBAL_CLAUDE_MD"
  fi

  # Inject slash commands
  COMMANDS_TARGET="$HOME/.claude/commands"
  if [ -d "$HARNESS_DIR/commands" ]; then
    mkdir -p "$COMMANDS_TARGET"
    for cmd_file in "$HARNESS_DIR/commands"/*.md; do
      [ -f "$cmd_file" ] || continue
      cmd_name=$(basename "$cmd_file")
      if [ ! -f "$COMMANDS_TARGET/$cmd_name" ]; then
        ln -sfn "$cmd_file" "$COMMANDS_TARGET/$cmd_name"
      fi
    done
    echo "agent-entrypoint: Injected agent harness commands → $COMMANDS_TARGET"
  fi
fi

apply_agentforge_context_envelope() {
  case "${AGENTFORGE_CONTEXT_INJECTION_ENABLED:-false}" in
    1|true|TRUE|yes|YES|on|ON) ;;
    *) return 0 ;;
  esac
  case "$CLI_TOOL" in
    claude|codex|gemini|opencode) CONTEXT_ADAPTER="$CLI_TOOL" ;;
    *) return 0 ;;
  esac
  if ! command -v agent-context-helper >/dev/null 2>&1; then
    echo "agent-entrypoint: WARNING: agent-context-helper not found — context envelope skipped"
    return 0
  fi

  CONTEXT_DIR="/tmp/agentforge-context"
  CONTEXT_ENVELOPE_PATH="${AGENTFORGE_CONTEXT_ENVELOPE_FILE:-$CONTEXT_DIR/envelope.json}"
  CONTEXT_REPORT_PATH="$CONTEXT_DIR/adapter-report.json"
  mkdir -p "$CONTEXT_DIR"

  if [ ! -f "$CONTEXT_ENVELOPE_PATH" ]; then
    CONTEXT_AGENT_ID="${AGENTFORGE_CONTEXT_AGENT_ID:-${AGENT_ID:-}}"
    if [ -z "${AGENTFORGE_CONTEXT_ENVELOPE_TOKEN:-}" ] \
      || [ -z "${AGENTFORGE_CONTEXT_TASK_ID:-}" ] \
      || [ -z "${AGENTFORGE_CONTEXT_RUN_ID:-}" ] \
      || [ -z "$CONTEXT_AGENT_ID" ] \
      || [ -z "${AGENTFORGE_SERVER_URL:-}" ]; then
      return 0
    fi

    CONTEXT_RESPONSE="$CONTEXT_DIR/envelope-response.json"
    CONTEXT_PAYLOAD=$(printf '{"agent_id":"%s","task_id":"%s","run_id":"%s","supported_versions":["v1"]}' \
      "$CONTEXT_AGENT_ID" "$AGENTFORGE_CONTEXT_TASK_ID" "$AGENTFORGE_CONTEXT_RUN_ID")

    if ! curl -fsS \
      -H "Authorization: Bearer ${AGENTFORGE_CONTEXT_ENVELOPE_TOKEN}" \
      -H "Content-Type: application/json" \
      -d "$CONTEXT_PAYLOAD" \
      "$AGENTFORGE_SERVER_URL/api/v1/context/envelope" \
      -o "$CONTEXT_RESPONSE"; then
      echo "agent-entrypoint: WARNING: context envelope fetch failed — continuing without injected context"
      return 0
    fi

    if ! node - "$CONTEXT_RESPONSE" "$CONTEXT_ENVELOPE_PATH" <<'NODE'
const fs = require('fs');
const body = JSON.parse(fs.readFileSync(process.argv[2], 'utf8'));
if (!body || body.ok !== true || !body.data) {
  process.exit(2);
}
fs.writeFileSync(process.argv[3], JSON.stringify(body.data, null, 2));
NODE
    then
      echo "agent-entrypoint: WARNING: context envelope response was not usable — continuing without injected context"
      return 0
    fi
  fi

  if agent-context-helper --adapter "$CONTEXT_ADAPTER" --envelope "$CONTEXT_ENVELOPE_PATH" --home "$HOME" --report "$CONTEXT_REPORT_PATH"; then
    echo "agent-entrypoint: Applied AgentForge context envelope for $CONTEXT_ADAPTER"
  else
    echo "agent-entrypoint: WARNING: $CONTEXT_ADAPTER context adapter failed — continuing without injected context"
  fi
}

apply_agentforge_context_envelope

# =============================================================================
# DevEnv resource helpers
# =============================================================================

read_cgroup_memory_limit_mb() {
  local raw=""

  # cgroup v2
  if [ -r /sys/fs/cgroup/memory.max ]; then
    raw=$(cat /sys/fs/cgroup/memory.max 2>/dev/null)
  # cgroup v1 fallback
  elif [ -r /sys/fs/cgroup/memory/memory.limit_in_bytes ]; then
    raw=$(cat /sys/fs/cgroup/memory/memory.limit_in_bytes 2>/dev/null)
  fi

  # "max" or very large sentinel values mean "unlimited"
  if [ -z "$raw" ] || [ "$raw" = "max" ]; then
    return 1
  fi
  if ! echo "$raw" | grep -Eq '^[0-9]+$'; then
    return 1
  fi
  if [ "$raw" -ge 900000000000000000 ]; then
    return 1
  fi

  echo $((raw / 1024 / 1024))
}

log_devenv_resource_budget() {
  local min_memory_mb="${AGENTFORGE_DEVENV_MIN_MEMORY_MB:-2048}"
  local memory_limit_mb=""

  if ! echo "$min_memory_mb" | grep -Eq '^[0-9]+$' || [ "$min_memory_mb" -le 0 ]; then
    echo "agent-entrypoint: WARNING: Invalid AGENTFORGE_DEVENV_MIN_MEMORY_MB=$min_memory_mb, using 2048MB"
    min_memory_mb=2048
  fi

  memory_limit_mb="$(read_cgroup_memory_limit_mb || true)"
  if [ -z "$memory_limit_mb" ]; then
    echo "agent-entrypoint: WARNING: DevEnv cgroup memory limit not detected (unlimited or unavailable)"
    return 0
  fi

  echo "agent-entrypoint: DevEnv memory limit=${memory_limit_mb}MB (recommended >= ${min_memory_mb}MB)"
  if [ "$memory_limit_mb" -lt "$min_memory_mb" ]; then
    echo "agent-entrypoint: WARNING: low memory budget may OOM-kill sidecar during docker build/compose"
    echo "agent-entrypoint: WARNING: raise container memory or lower build concurrency (COMPOSE_PARALLEL_LIMIT=$COMPOSE_PARALLEL_LIMIT)"
  fi
}

log_devenv_oom_diagnostics() {
  local events_file="/sys/fs/cgroup/memory.events"
  local oom_line=""
  local oom_kill_line=""
  local oom_kill_count=""

  if [ -r "$events_file" ]; then
    oom_line=$(grep '^oom ' "$events_file" 2>/dev/null)
    oom_kill_line=$(grep '^oom_kill ' "$events_file" 2>/dev/null)
    oom_kill_count=$(awk '/^oom_kill[[:space:]]+[0-9]+$/ {print $2}' "$events_file" 2>/dev/null)

    if [ -n "$oom_line" ]; then
      echo "agent-entrypoint: cgroup $oom_line"
    fi
    if [ -n "$oom_kill_line" ]; then
      echo "agent-entrypoint: cgroup $oom_kill_line"
    fi
    if [ -n "$oom_kill_count" ] && [ "$oom_kill_count" -gt 0 ]; then
      echo "agent-entrypoint: ERROR: detected cgroup OOM kill(s); Docker proxy sidecar likely terminated by memory pressure"
      echo "agent-entrypoint: ALERT: sidecar_oom_kill_total=$oom_kill_count"
    fi
  else
    echo "agent-entrypoint: WARNING: cannot read $events_file, OOM diagnosis unavailable"
  fi

  if [ -f /tmp/sidecar.log ]; then
    echo "agent-entrypoint: Sidecar log tail (last 20 lines):"
    tail -n 20 /tmp/sidecar.log | sed 's/^/agent-entrypoint: sidecar-log: /'
  fi
}

# =============================================================================
# DevEnv Docker Proxy setup
# =============================================================================
# When AGENTFORGE_DEVENV_POLICY is set, the sidecar starts a Docker-API-compatible
# proxy on a Unix socket. Configure DOCKER_HOST so the docker CLI uses it.
# "fail-closed" is a sentinel: sidecar starts a deny-all proxy.

if [ -n "$AGENTFORGE_DEVENV_POLICY" ]; then
  DEVENV_SOCKET="${AGENTFORGE_DOCKER_PROXY_SOCKET:-/tmp/docker-proxy.sock}"
  export DOCKER_HOST="unix://$DEVENV_SOCKET"
  if [ "$AGENTFORGE_DEVENV_POLICY" = "fail-closed" ]; then
    echo "agent-entrypoint: DevEnv Docker proxy in FAIL-CLOSED mode (DOCKER_HOST=$DOCKER_HOST)"
    echo "agent-entrypoint: All Docker commands will be denied — check server logs for policy load failure"
  else
    echo "agent-entrypoint: DevEnv Docker proxy enabled (DOCKER_HOST=$DOCKER_HOST)"
  fi
  # Force legacy Docker builder inside the agent container so that
  # `docker compose up --build` uses POST /build (handled by our proxy)
  # instead of Compose v2's internal buildx (which tries to create a
  # BuildKit container via gRPC — unsupported by NATS request-reply).
  # The platform side runs with DOCKER_BUILDKIT=1 for full BuildKit features.
  export DOCKER_BUILDKIT=0
  # The buildx CLI plugin handles direct `docker buildx build` calls.
  echo "agent-entrypoint: DevEnv build: DOCKER_BUILDKIT=0 (compose uses POST /build), buildx plugin available"
  # Cloud-native safety default: serialize compose build fan-out unless caller overrides.
  export COMPOSE_PARALLEL_LIMIT="${COMPOSE_PARALLEL_LIMIT:-1}"
  echo "agent-entrypoint: DevEnv compose parallel limit: COMPOSE_PARALLEL_LIMIT=$COMPOSE_PARALLEL_LIMIT"
  log_devenv_resource_budget
  # NOTE: The proxy socket is created by the sidecar, which starts later.
  # We only set DOCKER_HOST here; the socket wait happens after sidecar startup.
else
  echo "agent-entrypoint: WARNING: AGENTFORGE_DEVENV_POLICY is not set — Docker proxy will not start"
  echo "agent-entrypoint: WARNING: Docker commands (docker run, docker compose) will fail inside this container"
  echo "agent-entrypoint: WARNING: This usually means the server did not inject the policy — check session.service.ts"
fi

# =============================================================================
# Skip interactive prompts (onboarding, permissions dialog, folder trust)
# =============================================================================
# Claude CLI checks ~/.claude.json for multiple interactive state flags:
# - hasCompletedOnboarding / theme: skip first-run wizard
# - bypassPermissionsModeAccepted: skip --dangerously-skip-permissions dialog
# - projects./workspace.hasTrustDialogAccepted: skip "do you trust this folder?" prompt
# Pre-creating this state file bypasses all three prompts entirely.
# Claude Code rewrites .claude.json on each run, so the patching logic below
# re-injects keys on container restart.

if [ "$CLI_TOOL" = "claude" ]; then
  USER_STATE="$HOME/.claude.json"
  if [ ! -f "$USER_STATE" ]; then
    # Create state file with all bypass flags:
    # - hasCompletedOnboarding: skip theme/intro wizard
    # - bypassPermissionsModeAccepted: skip --dangerously-skip-permissions dialog
    # - projects./workspace.hasTrustDialogAccepted: skip "do you trust this folder?" prompt
    if command -v node &>/dev/null; then
      node -e "
        const fs = require('fs');
        fs.writeFileSync('$USER_STATE', JSON.stringify({
          hasCompletedOnboarding: true,
          theme: 'dark',
          bypassPermissionsModeAccepted: true,
          projects: {
            '/workspace': { hasTrustDialogAccepted: true, allowedTools: [] }
          }
        }));
      " 2>/dev/null && echo "agent-entrypoint: Created user state ($USER_STATE) — onboarding + bypass + folder trust"
    else
      echo '{"hasCompletedOnboarding":true,"theme":"dark","bypassPermissionsModeAccepted":true,"projects":{"/workspace":{"hasTrustDialogAccepted":true,"allowedTools":[]}}}' > "$USER_STATE" 2>/dev/null \
        && echo "agent-entrypoint: Created user state ($USER_STATE) — onboarding + bypass + folder trust"
    fi
    if [ ! -f "$USER_STATE" ]; then
      echo "agent-entrypoint: ERROR: Failed to write $USER_STATE — interactive prompts may block container"
    fi
  else
    # Claude Code rewrites .claude.json on each run, stripping our injected keys.
    # Ensure all bypass flags survive across container restarts so interactive
    # prompts don't block the container.
    if command -v node &>/dev/null; then
      node -e "
        const fs = require('fs');
        const d = JSON.parse(fs.readFileSync('$USER_STATE', 'utf8'));
        let changed = false;
        if (d.bypassPermissionsModeAccepted !== true) { d.bypassPermissionsModeAccepted = true; changed = true; }
        if (d.hasCompletedOnboarding !== true) { d.hasCompletedOnboarding = true; changed = true; }
        if (!d.projects) d.projects = {};
        if (!d.projects['/workspace'] || d.projects['/workspace'].hasTrustDialogAccepted !== true) {
          d.projects['/workspace'] = Object.assign(d.projects['/workspace'] || {}, { hasTrustDialogAccepted: true });
          changed = true;
        }
        if (changed) {
          fs.writeFileSync('$USER_STATE', JSON.stringify(d));
          console.log('agent-entrypoint: Patched ' + '$USER_STATE' + ' — ensured bypass + folder trust');
        }
      " 2>/dev/null
    fi
  fi
fi

# Codex: pre-create config.toml with file-based credential storage (containers lack OS keyring)
if [ "$CLI_TOOL" = "codex" ]; then
  CODEX_CONFIG="$HOME/.codex/config.toml"
  mkdir -p "$(dirname "$CODEX_CONFIG")"
  if [ ! -f "$CODEX_CONFIG" ]; then
    cat > "$CODEX_CONFIG" << 'TOML'
# Force file-based credential storage (containers lack OS keyring)
cli_auth_credentials_store = "file"
TOML
    echo "agent-entrypoint: Created codex config ($CODEX_CONFIG)"
  fi
  if ! grep -Eq '^[[:space:]]*codex_hooks[[:space:]]*=' "$CODEX_CONFIG"; then
    CODEX_CONFIG_TMP="$(mktemp)"
    awk '
      BEGIN { inserted = 0 }
      /^\[features\][[:space:]]*$/ && inserted == 0 {
        print
        print "codex_hooks = true"
        inserted = 1
        next
      }
      { print }
      END {
        if (inserted == 0) {
          print ""
          print "[features]"
          print "codex_hooks = true"
        }
      }
    ' "$CODEX_CONFIG" > "$CODEX_CONFIG_TMP" \
      && cat "$CODEX_CONFIG_TMP" > "$CODEX_CONFIG"
    rm -f "$CODEX_CONFIG_TMP"
    echo "agent-entrypoint: Enabled Codex hooks feature in $CODEX_CONFIG"
  fi
  # Skip first-run personality migration prompt
  touch "$HOME/.codex/.personality_migration" 2>/dev/null
fi

# Gemini: pre-create state to skip onboarding wizard and trust workspace
if [ "$CLI_TOOL" = "gemini" ]; then
  GEMINI_STATE="$HOME/.gemini/state.json"
  mkdir -p "$(dirname "$GEMINI_STATE")"
  if [ ! -f "$GEMINI_STATE" ]; then
    echo '{"hasCompletedOnboarding":true}' > "$GEMINI_STATE" 2>/dev/null
    echo "agent-entrypoint: Created gemini state to skip onboarding"
  fi
  # Pre-trust the workspace directory so the interactive trust dialog
  # doesn't block the container. Gemini CLI stores trust decisions in
  # trustedFolders.json with path → trust-level mapping.
  GEMINI_TRUST="$HOME/.gemini/trustedFolders.json"
  if [ ! -f "$GEMINI_TRUST" ]; then
    echo '{"/workspace":"TRUST_FOLDER","/":"TRUST_PARENT"}' > "$GEMINI_TRUST" 2>/dev/null
    chmod 600 "$GEMINI_TRUST" 2>/dev/null
    echo "agent-entrypoint: Pre-trusted /workspace for Gemini CLI"
  fi
fi

# OpenCode: inject API key into auth.json and configure provider
if [ "$CLI_TOOL" = "opencode" ]; then
  OPENCODE_AUTH="$HOME/.local/share/opencode/auth.json"
  OPENCODE_CONFIG="$HOME/.config/opencode/opencode.json"
  mkdir -p "$(dirname "$OPENCODE_AUTH")" "$(dirname "$OPENCODE_CONFIG")"

  # Inject ANTHROPIC_API_KEY into opencode's auth.json (opencode reads keys from here)
  if [ -n "${ANTHROPIC_API_KEY:-}" ] && [ ! -f "$OPENCODE_AUTH" ]; then
    cat > "$OPENCODE_AUTH" << EOF
{"anthropic":{"type":"api","key":"${ANTHROPIC_API_KEY}"}}
EOF
    chmod 600 "$OPENCODE_AUTH"
    echo "agent-entrypoint: Created opencode auth ($OPENCODE_AUTH) from ANTHROPIC_API_KEY"
  fi

  # Create default config with anthropic provider if not present.
  # IMPORTANT: "theme" MUST be a named theme (not "system"). The default "system"
  # theme queries the terminal background color via OSC 11 during startup. In a
  # headless container (no terminal emulator attached), the query blocks the Zig
  # renderer indefinitely, preventing the TUI from ever rendering.
  if [ ! -f "$OPENCODE_CONFIG" ]; then
    cat > "$OPENCODE_CONFIG" << 'JSON'
{
  "$schema": "https://opencode.ai/config.json",
  "theme": "catppuccin",
  "provider": {
    "anthropic": {}
  }
}
JSON
    echo "agent-entrypoint: Created opencode config ($OPENCODE_CONFIG)"
  fi
fi

# =============================================================================
# CLI version info
# =============================================================================
# CLI tool is baked into the image at build time with a pinned version.
# Updates happen via CI pipeline (new image build), not at runtime.
# See: docs/plans/2026-02-22-cloud-native-cli-update-design.md

echo "agent-entrypoint: $CLI_TOOL v${AGENTFORGE_CLI_VERSION:-unknown}"

# =============================================================================
# Pre-flight checks
# =============================================================================

if [ -z "$AGENTFORGE_SERVER_URL" ]; then
  echo "agent-entrypoint: WARNING: AGENTFORGE_SERVER_URL is not set — legacy HTTP dual-write disabled"
fi

# =============================================================================
# Start sidecar
# =============================================================================
# The sidecar (Rust binary) publishes events to NATS with WAL-backed durability.

RELAY_SOCKET="/tmp/agentforge-relay.sock"
RELAY_PID=""
RELAY_PID_FILE="/tmp/agentforge-sidecar.pid"
RELAY_STOP_MARKER="/tmp/agentforge-sidecar.stop"
RELAY_WATCHER_PID=""
SIDECAR_MAX_RESTARTS=5
SIDECAR_RESTART_COUNT=0

rm -f "$RELAY_PID_FILE" "$RELAY_STOP_MARKER"

# OOM score strategy: raise the CLI process's OOM score so the kernel prefers
# killing the memory-heavy CLI over the lightweight sidecar (~10 MB).
# Negative scores require CAP_SYS_RESOURCE (unavailable in unprivileged containers),
# but unprivileged processes CAN raise their own score (or children's).
# The sidecar keeps the default score (0); the CLI gets +500.
# If OOM still kills the sidecar despite this, the watchdog restarts it.
SIDECAR_OOM_CLI_SCORE=500

start_sidecar_process() {
  # Clean up stale sockets from previous instance before starting.
  # The Rust sidecar also removes stale sockets on startup, but doing it here
  # covers the race where the old socket lingers after an unclean exit.
  rm -f "$RELAY_SOCKET" 2>/dev/null
  [ -n "${DEVENV_SOCKET:-}" ] && rm -f "$DEVENV_SOCKET" 2>/dev/null
  /usr/local/bin/agentforge-sidecar >> /tmp/sidecar.log 2>&1 &
  RELAY_PID=$!
  echo "$RELAY_PID" > "$RELAY_PID_FILE"
  echo "agent-entrypoint: Started agentforge-sidecar (PID: $RELAY_PID)"
}

# start_sidecar_watcher runs a background supervisor loop that monitors the
# sidecar process and restarts it on unexpected exit (e.g., OOM kill, crash).
#
# Cloud-native design:
#   - Exponential backoff: 2s, 4s, 8s, 16s, 32s between restarts
#   - Circuit breaker: stops after SIDECAR_MAX_RESTARTS to avoid restart storms
#   - OOM diagnostics: reads cgroup memory.events on each failure
#   - Structured logging: machine-parseable ALERT lines for monitoring
#   - Socket cleanup: removes stale UDS before restart (start_sidecar_process)
#   - Graceful stop: RELAY_STOP_MARKER signals intentional shutdown
start_sidecar_watcher() {
  (
    restart_count=0
    backoff=2

    while true; do
      sleep "$backoff"

      # Intentional shutdown — exit watcher cleanly
      if [ -f "$RELAY_STOP_MARKER" ]; then
        exit 0
      fi

      current_pid=""
      if [ -f "$RELAY_PID_FILE" ]; then
        current_pid=$(cat "$RELAY_PID_FILE" 2>/dev/null)
      fi

      if [ -z "$current_pid" ]; then
        continue
      fi

      # Sidecar still running — reset backoff on sustained health
      if kill -0 "$current_pid" 2>/dev/null; then
        # Reset backoff after 60s of stable uptime (30 successful checks at 2s)
        backoff=2
        restart_count=0
        continue
      fi

      # --- Sidecar died unexpectedly ---
      restart_count=$((restart_count + 1))
      echo "agent-entrypoint: ERROR: sidecar exited unexpectedly (pid=$current_pid, restart=$restart_count/$SIDECAR_MAX_RESTARTS)"
      echo "agent-entrypoint: ALERT: sidecar_unexpected_exit_total=$restart_count"
      log_devenv_oom_diagnostics

      # Circuit breaker: stop restarting after max attempts
      if [ "$restart_count" -ge "$SIDECAR_MAX_RESTARTS" ]; then
        echo "agent-entrypoint: ERROR: sidecar restart limit reached ($SIDECAR_MAX_RESTARTS) — giving up"
        echo "agent-entrypoint: ERROR: Docker proxy and event relay permanently unavailable for this session"
        echo "agent-entrypoint: ALERT: sidecar_circuit_breaker_open=1"
        exit 0
      fi

      # Restart with backoff
      echo "agent-entrypoint: Restarting sidecar (attempt $restart_count, backoff=${backoff}s)..."
      rm -f "$RELAY_SOCKET" 2>/dev/null
      [ -n "${DEVENV_SOCKET:-}" ] && rm -f "$DEVENV_SOCKET" 2>/dev/null
      /usr/local/bin/agentforge-sidecar >> /tmp/sidecar.log 2>&1 &
      new_pid=$!
      echo "$new_pid" > "$RELAY_PID_FILE"
      echo "agent-entrypoint: Sidecar restarted (PID: $new_pid)"

      # Wait for sockets to come up (max 10s)
      socket_wait=0
      while [ ! -S "$RELAY_SOCKET" ] && [ "$socket_wait" -lt 100 ]; do
        sleep 0.1
        socket_wait=$((socket_wait + 1))
      done

      if [ -S "$RELAY_SOCKET" ]; then
        echo "agent-entrypoint: Sidecar relay socket restored at $RELAY_SOCKET"
      else
        echo "agent-entrypoint: WARNING: Sidecar relay socket not ready after restart"
      fi

      if [ -n "${DEVENV_SOCKET:-}" ]; then
        socket_wait=0
        while [ ! -S "$DEVENV_SOCKET" ] && [ "$socket_wait" -lt 100 ]; do
          sleep 0.1
          socket_wait=$((socket_wait + 1))
        done
        if [ -S "$DEVENV_SOCKET" ]; then
          echo "agent-entrypoint: Docker proxy socket restored at $DEVENV_SOCKET"
        else
          echo "agent-entrypoint: WARNING: Docker proxy socket not ready after restart"
        fi
      fi

      # Exponential backoff: 2 → 4 → 8 → 16 → 32 (capped)
      if [ "$backoff" -lt 32 ]; then
        backoff=$((backoff * 2))
      fi
    done
  ) &
  RELAY_WATCHER_PID=$!
}

# Pool-managed containers skip sidecar startup at entrypoint time.
# The sidecar is started later by ContainerRuntime.injectSessionEnv()
# via docker exec when a session is allocated to this container.
if [ "$AGENTFORGE_POOL_MANAGED" = "true" ]; then
  echo "agent-entrypoint: Pool-managed container — sidecar will start at session allocation"
elif [ -x /usr/local/bin/agentforge-sidecar ] && [ -n "$AGENTFORGE_NATS_URL" ]; then
  # Redirect sidecar output to a log file to prevent it from polluting the
  # container's PTY stream. In TTY mode, stderr merges with stdout on the same
  # PTY — sidecar JSON logs would appear on the TUI screen and corrupt VT100
  # screen captures used for response extraction.
  start_sidecar_process

  RELAY_WAIT=0
  while [ ! -S "$RELAY_SOCKET" ] && [ "$RELAY_WAIT" -lt 50 ]; do
    sleep 0.1
    RELAY_WAIT=$((RELAY_WAIT + 1))
  done

  if [ -S "$RELAY_SOCKET" ]; then
    echo "agent-entrypoint: Sidecar relay socket ready at $RELAY_SOCKET"
  else
    echo "agent-entrypoint: WARNING: Sidecar relay socket not ready after 5s — events will be lost"
  fi

  # Wait for Docker proxy socket (created by sidecar when AGENTFORGE_DEVENV_POLICY is set)
  if [ -n "${DEVENV_SOCKET:-}" ]; then
    echo "agent-entrypoint: Waiting for Docker proxy socket..."
    DEVENV_WAIT=0
    while [ ! -S "$DEVENV_SOCKET" ] && [ "$DEVENV_WAIT" -lt 100 ]; do
      sleep 0.1
      DEVENV_WAIT=$((DEVENV_WAIT + 1))
    done
    if [ -S "$DEVENV_SOCKET" ]; then
      echo "agent-entrypoint: Docker proxy socket ready"
    else
      echo "agent-entrypoint: ERROR: Docker proxy socket not ready after 10s — Docker commands will fail"
    fi
  fi

  # Start background supervisor — monitors sidecar health and auto-restarts on
  # unexpected exit (OOM kill, crash). See start_sidecar_watcher() for details.
  start_sidecar_watcher
else
  echo "agent-entrypoint: WARNING: Sidecar not available (missing binary or AGENTFORGE_NATS_URL) — events will be lost"
fi

# =============================================================================
# Start CLI agent
# =============================================================================
# Container stays alive as long as the CLI process runs.
# Prompts are delivered via stdin from ContainerRuntime.sendPrompt().

if [ $# -gt 0 ]; then
  CMD="$*"
  CMD_BIN="$1"
else
  CMD="$DEFAULT_CMD"
  # Extract the first word (binary name) from DEFAULT_CMD
  CMD_BIN="${DEFAULT_CMD%% *}"
  if [ "$CLI_TOOL" = "codex" ] && ! codex --help 2>&1 | grep -q -- "--yolo"; then
    CMD="$DEFAULT_CMD_FALLBACK"
  fi
fi

# Validate command exists before exec
if ! command -v "$CMD_BIN" &> /dev/null; then
  echo "agent-entrypoint: FATAL: '$CMD_BIN' not found in PATH"
  echo "agent-entrypoint: PATH=$PATH"
  exit 127
fi

echo "agent-entrypoint: Starting: $CMD"
echo "agent-entrypoint: Working directory: $(pwd)"
echo "agent-entrypoint: AGENTFORGE_SESSION_ID=${AGENTFORGE_SESSION_ID:-<unset>}"

# Docker creates the container with Tty: true, providing a real PTY (pts/0).
# The Dockerfile uses `tini -g` as PID 1 (proper init process). tini provides:
#   - Proper signal forwarding to the entire process group (-g flag)
#   - Zombie process reaping
#   - On `docker stop`: SIGTERM is forwarded to ALL children (sidecar + CLI)
#
# The Rust sidecar is expected to shut down cleanly with the container and
# flush WAL-backed event state before exit.
#
# `script` wraps the CLI to preserve PTY (isTTY = true) in the child process.
# Running in the FOREGROUND means:
#   - stdin is inherited from the Docker PTY naturally (no FD hacks)
#   - SIGWINCH is forwarded to the child PTY automatically by `script`
#   - When the CLI exits, bash can clean up the sidecar before exiting
if [ -n "$RELAY_PID" ]; then
  # TUI tools: `script` creates a child PTY (pts/1) that starts at 0x0.
  # Set a default size on pts/0 so script inherits it for pts/1.
  # Without this, TUI apps cannot render (0x0 terminal = invisible).
  # The browser terminal sends a resize via Docker on attach, but that
  # arrives AFTER the TUI has already initialized with 0x0.
  if [ "$CLI_MODE" = "tui" ]; then
    stty cols 120 rows 40 2>/dev/null
    # Native library extraction: TUI tools like OpenCode use Zig-compiled .so
    # libraries that are extracted to TMPDIR and dlopen()'d at startup. The main
    # /tmp is mounted with noexec (CIS compliance), so we redirect TMPDIR to
    # /run/exec-tmp which allows execution. Without this, OpenTUI fails with:
    #   "Failed to open library ... failed to map segment from shared object"
    export TMPDIR=/run/exec-tmp
  fi

  # Raise this shell's OOM score so the kernel prefers killing the CLI process
  # tree over the lightweight sidecar. Children (script → CLI) inherit the score.
  # Unprivileged processes can only raise (never lower) their own oom_score_adj.
  if echo "$SIDECAR_OOM_CLI_SCORE" > /proc/self/oom_score_adj 2>/dev/null; then
    echo "agent-entrypoint: CLI OOM score raised (oom_score_adj=$SIDECAR_OOM_CLI_SCORE) — sidecar protected"
  fi

  # Write PID file for the command subscriber (cancel_task sends SIGINT).
  # After exec (below in the no-sidecar path) this would be script's PID.
  # Here $$ is bash's PID; script inherits the process group so SIGINT
  # sent to this PID reaches the CLI via script's PTY forwarding.
  echo "$$" > /tmp/cli.pid

  # Run CLI in foreground — `script` inherits stdin from Docker PTY directly.
  # No `&` means no bash stdin-to-/dev/null redirect. No FD hacks needed.
  script -qefc "$CMD" /dev/null
  CLI_EXIT=$?

  # CLI exited. Signal the sidecar to flush WAL and shut down.
  # (On `docker stop`, sidecar already received SIGTERM from tini -g.)
  touch "$RELAY_STOP_MARKER" 2>/dev/null
  if [ -n "$RELAY_WATCHER_PID" ]; then
    kill "$RELAY_WATCHER_PID" 2>/dev/null
    wait "$RELAY_WATCHER_PID" 2>/dev/null
  fi

  ACTIVE_RELAY_PID=""
  if [ -f "$RELAY_PID_FILE" ]; then
    ACTIVE_RELAY_PID=$(cat "$RELAY_PID_FILE" 2>/dev/null)
  fi
  if [ -z "$ACTIVE_RELAY_PID" ]; then
    ACTIVE_RELAY_PID="$RELAY_PID"
  fi
  if [ -n "$ACTIVE_RELAY_PID" ]; then
    kill "$ACTIVE_RELAY_PID" 2>/dev/null
    wait "$ACTIVE_RELAY_PID" 2>/dev/null
  fi
  rm -f "$RELAY_PID_FILE" "$RELAY_STOP_MARKER"
  exit $CLI_EXIT
else
  # No sidecar — exec directly (replaces shell, no cleanup needed).
  exec script -qefc "$CMD" /dev/null
fi

#!/usr/bin/env bash
set -Eeuo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
tmp_home="$(mktemp -d /tmp/agentforge-real-cli-home.XXXXXX)"
cleaned=0

cleanup() {
  if [ "$cleaned" -eq 1 ]; then
    return 0
  fi
  cleaned=1

  local attempt
  for attempt in 1 2 3; do
    chmod -R u+rwX "$tmp_home" 2>/dev/null || true
    rm -rf "$tmp_home" 2>/dev/null || true
    if [ ! -e "$tmp_home" ]; then
      return 0
    fi
    sleep "$attempt"
  done

  echo "ERROR: failed to remove temporary real CLI credential HOME: $tmp_home" >&2
  return 1
}

trap 'status=$?; cleanup || status=$?; exit "$status"' EXIT INT TERM

usage() {
  cat <<'USAGE'
Run the orchestration real-user E2E against a live deployment and invoke a real CLI.

Required:
  E2E_DATABASE_URL     PostgreSQL URL for cleanup and fixture seeding

Optional:
  BASE_URL                         default: http://127.0.0.1:4007
  E2E_EMAIL                        default: dev@example.com
  E2E_PASSWORD                     required
  ORCHESTRATION_REAL_CLI_TOOL      codex|claude|gemini|opencode, default: codex
  ORCHESTRATION_REAL_CLI_MODEL     default for codex: gpt-5.5
  ORCHESTRATION_REAL_CLI_EXECUTION_MODE  container|host, default: container
  AGENTFORGE_SIDECAR_CONTAINER_IMAGE     default: agentforge-agent:<tool> in container mode
  ORCHESTRATION_REAL_CLI_SOURCE_HOME  source credential home, default: $HOME
  ORCHESTRATION_REAL_CLI_COPY_ALL_CREDS  set to 1 to copy all supported local CLI credential dirs
  ORCHESTRATION_REAL_CLI_E2E_TIMEOUT  default: 300s
  ORCHESTRATION_REAL_CLI_TASK_TIMEOUT_MS  default: 300000 for real CLI mode
  ORCHESTRATION_REAL_SIDECAR_START_TIMEOUT_MS  default: 60000
  AGENTFORGE_SIDECAR_BIN           default: rust/target/debug/agentforge-sidecar in host mode
  NATS_PORT                        default: 4222

The script copies only local CLI credential directories into a temporary 0700
HOME, points the sidecar at that HOME, and fails if cleanup cannot remove it.
Container mode runs /usr/local/bin/agentforge-sidecar inside the agent image,
so the selected CLI is the one baked into the production agent container.
USAGE
}

if [ "${1:-}" = "-h" ] || [ "${1:-}" = "--help" ]; then
  usage
  exit 0
fi

BASE_URL="${BASE_URL:-http://127.0.0.1:4007}"
E2E_EMAIL="${E2E_EMAIL:-dev@example.com}"
ORCHESTRATION_REAL_CLI_TOOL="${ORCHESTRATION_REAL_CLI_TOOL:-codex}"
if [ -z "${ORCHESTRATION_REAL_CLI_MODEL+x}" ] && [ "$ORCHESTRATION_REAL_CLI_TOOL" = "codex" ]; then
  ORCHESTRATION_REAL_CLI_MODEL="gpt-5.5"
else
  ORCHESTRATION_REAL_CLI_MODEL="${ORCHESTRATION_REAL_CLI_MODEL:-}"
fi
ORCHESTRATION_REAL_CLI_EXECUTION_MODE="${ORCHESTRATION_REAL_CLI_EXECUTION_MODE:-container}"
AGENTFORGE_SIDECAR_CONTAINER_IMAGE="${AGENTFORGE_SIDECAR_CONTAINER_IMAGE:-agentforge-agent:${ORCHESTRATION_REAL_CLI_TOOL}}"
ORCHESTRATION_REAL_CLI_SOURCE_HOME="${ORCHESTRATION_REAL_CLI_SOURCE_HOME:-$HOME}"
ORCHESTRATION_REAL_CLI_E2E_TIMEOUT="${ORCHESTRATION_REAL_CLI_E2E_TIMEOUT:-300s}"
AGENTFORGE_SIDECAR_BIN="${AGENTFORGE_SIDECAR_BIN:-$repo_root/rust/target/debug/agentforge-sidecar}"
NATS_PORT="${NATS_PORT:-4222}"

resolve_database_url_for_host() {
  local url="$1"
  local db_host
  db_host="$(E2E_DATABASE_URL="$url" node -e 'process.stdout.write(new URL(process.env.E2E_DATABASE_URL).hostname)')"
  if getent hosts "$db_host" >/dev/null 2>&1; then
    printf '%s' "$url"
    return 0
  fi
  if ! command -v docker >/dev/null 2>&1; then
    printf '%s' "$url"
    return 0
  fi

  local published
  published="$(
    docker inspect "$db_host" \
      --format '{{with index .NetworkSettings.Ports "5432/tcp"}}{{(index . 0).HostIp}}:{{(index . 0).HostPort}}{{end}}' \
      2>/dev/null || true
  )"
  if [ -z "$published" ] || [ "$published" = ":" ]; then
    printf '%s' "$url"
    return 0
  fi

  local host_ip="${published%:*}"
  local host_port="${published##*:}"
  if [ -z "$host_ip" ] || [ "$host_ip" = "0.0.0.0" ] || [ "$host_ip" = "::" ]; then
    host_ip="127.0.0.1"
  fi
  E2E_DATABASE_URL="$url" DB_HOST="$host_ip" DB_PORT="$host_port" node -e '
    const url = new URL(process.env.E2E_DATABASE_URL)
    url.hostname = process.env.DB_HOST
    url.port = process.env.DB_PORT
    process.stdout.write(url.toString())
  '
}

if [ -z "${E2E_DATABASE_URL:-}" ]; then
  echo "ERROR: E2E_DATABASE_URL is required" >&2
  usage >&2
  exit 2
fi
if [ -z "${E2E_PASSWORD:-}" ]; then
  echo "ERROR: E2E_PASSWORD is required for $E2E_EMAIL" >&2
  usage >&2
  exit 2
fi

case "$ORCHESTRATION_REAL_CLI_TOOL" in
  codex|claude|gemini|opencode) ;;
  *)
    echo "ERROR: ORCHESTRATION_REAL_CLI_TOOL must be codex, claude, gemini, or opencode" >&2
    exit 2
    ;;
esac

case "$ORCHESTRATION_REAL_CLI_EXECUTION_MODE" in
  container)
    if ! command -v docker >/dev/null 2>&1; then
      echo "ERROR: docker is required for container real-CLI E2E mode" >&2
      exit 2
    fi
    if ! docker image inspect "$AGENTFORGE_SIDECAR_CONTAINER_IMAGE" >/dev/null 2>&1; then
      echo "ERROR: missing agent container image: $AGENTFORGE_SIDECAR_CONTAINER_IMAGE" >&2
      echo "Build or pull it before running the real-CLI E2E." >&2
      exit 2
    fi
    ;;
  host)
    if ! command -v "$ORCHESTRATION_REAL_CLI_TOOL" >/dev/null 2>&1; then
      echo "ERROR: CLI tool '$ORCHESTRATION_REAL_CLI_TOOL' is not on PATH" >&2
      exit 2
    fi
    if [ ! -x "$AGENTFORGE_SIDECAR_BIN" ]; then
      echo "ERROR: sidecar binary is missing or not executable: $AGENTFORGE_SIDECAR_BIN" >&2
      echo "Build it first with: cargo build -p agentforge-sidecar" >&2
      exit 2
    fi
    ;;
  *)
    echo "ERROR: ORCHESTRATION_REAL_CLI_EXECUTION_MODE must be container or host" >&2
    exit 2
    ;;
esac

chmod 700 "$tmp_home"
credential_dirs=()
if [ "${ORCHESTRATION_REAL_CLI_COPY_ALL_CREDS:-0}" = "1" ]; then
  credential_dirs=(.codex .claude .gemini .local/share/opencode)
else
  case "$ORCHESTRATION_REAL_CLI_TOOL" in
    codex) credential_dirs=(.codex) ;;
    claude) credential_dirs=(.claude) ;;
    gemini) credential_dirs=(.gemini) ;;
    opencode) credential_dirs=(.local/share/opencode) ;;
  esac
fi

for dir in "${credential_dirs[@]}"; do
  if [ -d "$ORCHESTRATION_REAL_CLI_SOURCE_HOME/$dir" ]; then
    if [ "$dir" = ".codex" ]; then
      mkdir -p "$tmp_home/.codex"
      for item in auth.json auth.json2 installation_id version.json models_cache.json; do
        if [ -e "$ORCHESTRATION_REAL_CLI_SOURCE_HOME/.codex/$item" ]; then
          cp -a "$ORCHESTRATION_REAL_CLI_SOURCE_HOME/.codex/$item" "$tmp_home/.codex/$item"
        fi
      done
    elif [ "$dir" = ".claude" ]; then
      mkdir -p "$tmp_home/.claude"
      for item in .credentials.json credentials.json settings.json settings.local.json; do
        if [ -e "$ORCHESTRATION_REAL_CLI_SOURCE_HOME/.claude/$item" ]; then
          cp -a "$ORCHESTRATION_REAL_CLI_SOURCE_HOME/.claude/$item" "$tmp_home/.claude/$item"
        fi
      done
    elif [ "$dir" = ".gemini" ]; then
      mkdir -p "$tmp_home/.gemini"
      for item in oauth_creds.json google_accounts.json settings.json state.json trustedFolders.json installation_id projects.json; do
        if [ -e "$ORCHESTRATION_REAL_CLI_SOURCE_HOME/.gemini/$item" ]; then
          cp -a "$ORCHESTRATION_REAL_CLI_SOURCE_HOME/.gemini/$item" "$tmp_home/.gemini/$item"
        fi
      done
    else
      mkdir -p "$tmp_home/$(dirname "$dir")"
      cp -a "$ORCHESTRATION_REAL_CLI_SOURCE_HOME/$dir" "$tmp_home/$dir"
    fi
  fi
done
if [ -f "$ORCHESTRATION_REAL_CLI_SOURCE_HOME/.claude.json" ]; then
  cp -a "$ORCHESTRATION_REAL_CLI_SOURCE_HOME/.claude.json" "$tmp_home/.claude.json"
fi
chmod -R go-rwx "$tmp_home" 2>/dev/null || true

case "$ORCHESTRATION_REAL_CLI_TOOL" in
  codex)
    if [ ! -d "$tmp_home/.codex" ]; then
      echo "ERROR: missing source Codex credentials at $ORCHESTRATION_REAL_CLI_SOURCE_HOME/.codex" >&2
      exit 2
    fi
    if [ ! -f "$tmp_home/.codex/auth.json" ] && [ ! -f "$tmp_home/.codex/auth.json2" ]; then
      echo "ERROR: missing Codex auth file in $ORCHESTRATION_REAL_CLI_SOURCE_HOME/.codex" >&2
      exit 2
    fi
    ;;
  claude)
    if [ ! -d "$tmp_home/.claude" ]; then
      echo "ERROR: missing source Claude credentials at $ORCHESTRATION_REAL_CLI_SOURCE_HOME/.claude" >&2
      exit 2
    fi
    if [ ! -f "$tmp_home/.claude/.credentials.json" ] && [ ! -f "$tmp_home/.claude/credentials.json" ] && [ ! -f "$tmp_home/.claude.json" ]; then
      echo "ERROR: missing Claude auth files in $ORCHESTRATION_REAL_CLI_SOURCE_HOME/.claude or $ORCHESTRATION_REAL_CLI_SOURCE_HOME/.claude.json" >&2
      exit 2
    fi
    ;;
  gemini)
    if [ ! -d "$tmp_home/.gemini" ]; then
      echo "ERROR: missing source Gemini credentials at $ORCHESTRATION_REAL_CLI_SOURCE_HOME/.gemini" >&2
      exit 2
    fi
    if [ ! -f "$tmp_home/.gemini/oauth_creds.json" ] && [ ! -f "$tmp_home/.gemini/google_accounts.json" ]; then
      echo "ERROR: missing Gemini auth files in $ORCHESTRATION_REAL_CLI_SOURCE_HOME/.gemini" >&2
      exit 2
    fi
    ;;
  opencode)
    if [ ! -d "$tmp_home/.local/share/opencode" ]; then
      echo "ERROR: missing source OpenCode credentials at $ORCHESTRATION_REAL_CLI_SOURCE_HOME/.local/share/opencode" >&2
      exit 2
    fi
    if [ ! -f "$tmp_home/.local/share/opencode/auth.json" ]; then
      echo "ERROR: missing OpenCode auth file in $ORCHESTRATION_REAL_CLI_SOURCE_HOME/.local/share/opencode" >&2
      exit 2
    fi
    ;;
esac

cd "$repo_root"

rm -f tests/e2e/.auth/user.json

ORCHESTRATION_REAL_E2E=1 \
ORCHESTRATION_REAL_CLI_E2E=1 \
ORCHESTRATION_REAL_CLI_TOOL="$ORCHESTRATION_REAL_CLI_TOOL" \
ORCHESTRATION_REAL_CLI_MODEL="$ORCHESTRATION_REAL_CLI_MODEL" \
ORCHESTRATION_REAL_CLI_HOME="$tmp_home" \
AGENTFORGE_SIDECAR_CONTAINER_IMAGE="$(
  if [ "$ORCHESTRATION_REAL_CLI_EXECUTION_MODE" = "container" ]; then
    printf '%s' "$AGENTFORGE_SIDECAR_CONTAINER_IMAGE"
  fi
)" \
ORCHESTRATION_REAL_E2E_CLEANUP_AUTH=1 \
BASE_URL="$BASE_URL" \
E2E_DATABASE_URL="$(resolve_database_url_for_host "$E2E_DATABASE_URL")" \
E2E_EMAIL="$E2E_EMAIL" \
E2E_PASSWORD="$E2E_PASSWORD" \
AGENTFORGE_SIDECAR_BIN="$AGENTFORGE_SIDECAR_BIN" \
NATS_PORT="$NATS_PORT" \
timeout --kill-after=30s "$ORCHESTRATION_REAL_CLI_E2E_TIMEOUT" npx playwright test \
  --config tests/e2e/playwright.config.ts \
  tests/e2e/specs/orchestration-real-task.spec.ts \
  --project chromium \
  --reporter list

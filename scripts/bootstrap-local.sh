#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ENV_FILE="${ENV_FILE:-$ROOT_DIR/docker/.env}"
ENV_EXAMPLE="${ENV_EXAMPLE:-$ROOT_DIR/docker/.env.example}"
CHECK_ONLY=0
START_STACK=0

usage() {
  cat <<'USAGE'
Prepare a local Wisdoverse Forge developer/self-host environment.

Usage:
  scripts/bootstrap-local.sh          Create/fill docker/.env when safe
  scripts/bootstrap-local.sh --check  Check prerequisites and env only
  scripts/bootstrap-local.sh --start  Prepare env, then run make dev-d

The script never overwrites non-empty values. If the env file already looks like
an external or production deployment, it reports the missing local values and
leaves the file unchanged.
USAGE
}

log() {
  printf '[bootstrap] %s\n' "$*"
}

warn() {
  printf '[bootstrap] WARNING: %s\n' "$*" >&2
}

die() {
  printf '[bootstrap] ERROR: %s\n' "$*" >&2
  exit 1
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --check)
      CHECK_ONLY=1
      ;;
    --start)
      START_STACK=1
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      usage >&2
      die "unknown argument: $1"
      ;;
  esac
  shift
done

require_cmd() {
  if ! command -v "$1" >/dev/null 2>&1; then
    die "missing required command: $1"
  fi
}

random_hex() {
  local bytes="$1"
  if command -v openssl >/dev/null 2>&1; then
    openssl rand -hex "$bytes"
    return
  fi
  od -An -N "$bytes" -tx1 /dev/urandom | tr -d ' \n'
}

random_base64() {
  local bytes="$1"
  if command -v openssl >/dev/null 2>&1; then
    openssl rand -base64 "$bytes" | tr -d '\n'
    return
  fi
  random_hex "$bytes"
}

env_value() {
  local key="$1"
  [ -f "$ENV_FILE" ] || return 0
  sed -n "s/^${key}=//p" "$ENV_FILE" | tail -n 1 | tr -d '\r'
}

set_env_value() {
  local key="$1"
  local value="$2"
  local tmp

  tmp="$(mktemp)"
  awk -v key="$key" -v value="$value" '
    BEGIN { done = 0 }
    $0 ~ "^" key "=" {
      print key "=" value
      done = 1
      next
    }
    { print }
    END {
      if (done == 0) {
        print key "=" value
      }
    }
  ' "$ENV_FILE" > "$tmp"
  mv "$tmp" "$ENV_FILE"
  chmod 600 "$ENV_FILE"
}

ensure_env_value() {
  local key="$1"
  local value="$2"
  local current

  current="$(env_value "$key")"
  if [ -n "$current" ]; then
    return 0
  fi
  if [ "$CHECK_ONLY" -eq 1 ] || [ "$WRITE_ALLOWED" -eq 0 ]; then
    MISSING_VALUES="${MISSING_VALUES}${key} "
    return 0
  fi
  set_env_value "$key" "$value"
  UPDATED_VALUES="${UPDATED_VALUES}${key} "
}

port_in_use() {
  local port="$1"
  if command -v lsof >/dev/null 2>&1; then
    lsof -nP -iTCP:"$port" -sTCP:LISTEN >/dev/null 2>&1
    return $?
  fi
  if command -v ss >/dev/null 2>&1; then
    ss -ltn | awk '{print $4}' | grep -Eq "[:.]${port}$"
    return $?
  fi
  return 1
}

generate_nats_material() {
  local nats_tmp
  nats_tmp="$(mktemp -d)"
  NATS_BOOTSTRAP_TMP="$nats_tmp"
  trap 'rm -rf "${NATS_BOOTSTRAP_TMP:-}"' EXIT

  run_nk() {
    if command -v nk >/dev/null 2>&1; then
      (cd "$nats_tmp" && nk "$@")
      return
    fi
    require_cmd docker
    docker run --rm -v "$nats_tmp:/work" -w /work natsio/nats-box:latest nk "$@"
  }

  run_nk -gen account > "$nats_tmp/issuer.seed"
  run_nk -gen account > "$nats_tmp/account-signing.seed"
  if ! run_nk -gen curve > "$nats_tmp/xkey.seed"; then
    if ! run_nk -gen x25519 > "$nats_tmp/xkey.seed"; then
      run_nk -gen xkey > "$nats_tmp/xkey.seed"
    fi
  fi

  NATS_ISSUER_SEED="$(cat "$nats_tmp/issuer.seed")"
  NATS_ISSUER_PUBLIC="$(run_nk -inkey issuer.seed -pubout)"
  NATS_ACCOUNT_SIGNING_SEED="$(cat "$nats_tmp/account-signing.seed")"
  NATS_XKEY_SEED="$(cat "$nats_tmp/xkey.seed")"
  NATS_XKEY_PUBLIC="$(run_nk -inkey xkey.seed -pubout)"
  rm -rf "$nats_tmp"
  NATS_BOOTSTRAP_TMP=""
}

looks_like_deployment_env() {
  local profiles database_url app_url external_network
  profiles="$(env_value COMPOSE_PROFILES)"
  database_url="$(env_value DATABASE_URL)"
  app_url="$(env_value APP_URL)"
  external_network="$(env_value EXTERNAL_NETWORK)"

  case ",${profiles}," in
    *,external,*|*,prod,*) return 0 ;;
  esac
  [ -n "$database_url" ] && return 0
  case "$app_url" in
    http://*|https://*) return 0 ;;
  esac
  [ -n "$external_network" ] && [ "$external_network" != "external-network" ] && return 0
  return 1
}

require_cmd git
require_cmd make
require_cmd docker
require_cmd npm
require_cmd node

if ! docker compose version >/dev/null 2>&1; then
  die "Docker Compose v2 is required: docker compose version failed"
fi

NODE_MAJOR="$(node -p "Number(process.versions.node.split('.')[0])" 2>/dev/null || printf '0')"
if [ "$NODE_MAJOR" -lt 24 ]; then
  die "Node.js 24+ is required; current major version is ${NODE_MAJOR:-unknown}"
fi

WRITE_ALLOWED=1
CREATED_ENV=0
UPDATED_VALUES=""
MISSING_VALUES=""

if [ ! -f "$ENV_FILE" ]; then
  [ -f "$ENV_EXAMPLE" ] || die "missing $ENV_EXAMPLE"
  if [ "$CHECK_ONLY" -eq 1 ]; then
    WRITE_ALLOWED=0
    warn "$ENV_FILE is missing"
  else
    mkdir -p "$(dirname "$ENV_FILE")"
    cp "$ENV_EXAMPLE" "$ENV_FILE"
    chmod 600 "$ENV_FILE"
    CREATED_ENV=1
    log "created $ENV_FILE from $ENV_EXAMPLE"
  fi
elif looks_like_deployment_env && [ "${BOOTSTRAP_LOCAL_ALLOW_DEPLOYMENT:-0}" -ne 1 ]; then
  WRITE_ALLOWED=0
  warn "$ENV_FILE looks deployment-specific; leaving it unchanged"
fi

if [ -f "$ENV_FILE" ]; then
  NEED_NATS=0
  for key in \
    NATS_BACKEND_PASSWORD \
    NATS_AUTH_SERVICE_PASSWORD \
    NATS_SYS_PASSWORD \
    NATS_CALLOUT_ISSUER_SEED \
    NATS_CALLOUT_ACCOUNT_SIGNING_KEY_SEED \
    NATS_CALLOUT_XKEY_SEED \
    NATS_CALLOUT_ISSUER_PUBLIC \
    NATS_CALLOUT_XKEY_PUBLIC
  do
    if [ -z "$(env_value "$key")" ]; then
      NEED_NATS=1
    fi
  done

  if [ "$NEED_NATS" -eq 1 ] && [ "$CHECK_ONLY" -eq 0 ] && [ "$WRITE_ALLOWED" -eq 1 ]; then
    log "generating local NATS callout key material"
    generate_nats_material
  fi

  ensure_env_value POSTGRES_PASSWORD "$(random_hex 24)"
  ensure_env_value REDIS_PASSWORD "$(random_hex 24)"
  ensure_env_value JWT_SECRET "$(random_base64 64)"
  ensure_env_value MCP_TOKEN "$(random_hex 32)"
  ensure_env_value API_KEY_SALT "$(random_base64 32)"
  ensure_env_value LLM_ENCRYPTION_KEY "$(random_hex 32)"
  ensure_env_value NATS_BACKEND_PASSWORD "$(random_hex 32)"
  ensure_env_value NATS_AUTH_SERVICE_PASSWORD "$(random_hex 32)"
  ensure_env_value NATS_SYS_PASSWORD "$(random_hex 32)"
  ensure_env_value NATS_SERVER_NAME "agentforge-primary"
  ensure_env_value NATS_CALLOUT_ISSUER_SEED "${NATS_ISSUER_SEED:-}"
  ensure_env_value NATS_CALLOUT_ACCOUNT_SIGNING_KEY_SEED "${NATS_ACCOUNT_SIGNING_SEED:-}"
  ensure_env_value NATS_CALLOUT_XKEY_SEED "${NATS_XKEY_SEED:-}"
  ensure_env_value NATS_CALLOUT_ISSUER_PUBLIC "${NATS_ISSUER_PUBLIC:-}"
  ensure_env_value NATS_CALLOUT_XKEY_PUBLIC "${NATS_XKEY_PUBLIC:-}"
fi

if [ "$CREATED_ENV" -eq 1 ]; then
  log "local env file is ready"
fi

if [ -n "$UPDATED_VALUES" ]; then
  log "filled local values: $UPDATED_VALUES"
fi

if [ -n "$MISSING_VALUES" ]; then
  warn "missing values: $MISSING_VALUES"
fi

if [ ! -d "$ROOT_DIR/node_modules" ]; then
  warn "node_modules is missing; run: npm install"
fi

for port in 4002 4003 4010 4222 5432 6379 7233 8233; do
  if port_in_use "$port"; then
    warn "port $port is already in use"
  fi
done

if [ "$START_STACK" -eq 1 ]; then
  if [ "$WRITE_ALLOWED" -eq 0 ] || [ -n "$MISSING_VALUES" ]; then
    die "not starting because local env preparation is incomplete"
  fi
  log "starting backend stack with make dev-d"
  (cd "$ROOT_DIR" && make dev-d)
fi

if [ "${BOOTSTRAP_LOCAL_QUIET_NEXT:-0}" -ne 1 ]; then
  cat <<'NEXT'

Next local commands:
  npm install          # only needed once, when node_modules is missing
  make quickstart-local # starts backend services and waits for health
  npm run dev          # starts the browser app on http://localhost:4002

Health checks:
  make local-health

Stop local services:
  make dev-down
NEXT
fi

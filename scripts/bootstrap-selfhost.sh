#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ENV_FILE="${ENV_FILE:-$ROOT_DIR/docker/.env}"
ENV_EXAMPLE="${ENV_EXAMPLE:-$ROOT_DIR/docker/.env.example}"
DOMAIN=""
CHECK_ONLY=0
WRITE_ALLOWED=1
CREATED_ENV=0
UPDATED_VALUES=""
MISSING_VALUES=""

usage() {
  cat <<'USAGE'
Prepare the self-contained production profile.

Usage:
  scripts/bootstrap-selfhost.sh
  scripts/bootstrap-selfhost.sh --check
  scripts/bootstrap-selfhost.sh --domain forge.example.com
  scripts/bootstrap-selfhost.sh --domain localhost
  HTTPS_PORT=18443 scripts/bootstrap-selfhost.sh --domain localhost

The prod profile uses Caddy. When --domain is omitted, the script uses APP_HOST
or APP_URL from the env file, falling back to localhost. Public domains receive
automatic HTTPS certificates when DNS points to this host and ports 80/443 are
reachable. Local/private trials use Caddy's internal local CA and may show a
browser warning unless that CA is trusted on the client machine. When HTTPS_PORT
is set to a non-443 value, APP_URL and CORS_ORIGIN include that public port.
USAGE
}

log() {
  printf '[selfhost] %s\n' "$*"
}

warn() {
  printf '[selfhost] WARNING: %s\n' "$*" >&2
}

die() {
  printf '[selfhost] ERROR: %s\n' "$*" >&2
  exit 1
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --check)
      CHECK_ONLY=1
      ;;
    --domain)
      shift
      [ "$#" -gt 0 ] || die "--domain requires a value"
      DOMAIN="$1"
      ;;
    --self-signed)
      warn "--self-signed is no longer required; Caddy manages local/private TLS"
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

env_or_file_value() {
  local key="$1"
  local value="${!key-}"
  if [ -n "$value" ]; then
    printf '%s' "$value"
    return
  fi
  env_value "$key"
}

normalize_host() {
  local value="$1"
  value="${value#http://}"
  value="${value#https://}"
  value="${value%%/*}"
  value="${value#[}"
  value="${value%]}"
  case "$value" in
    *:*) value="${value%%:*}" ;;
  esac
  printf '%s' "$value"
}

public_url() {
  local domain="$1"
  local https_port

  https_port="$(env_or_file_value HTTPS_PORT)"
  if [ -n "$https_port" ] && [ "$https_port" != "443" ]; then
    printf 'https://%s:%s' "$domain" "$https_port"
  else
    printf 'https://%s' "$domain"
  fi
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

set_env_if_empty_or_default() {
  local key="$1"
  local value="$2"
  local default_value="$3"
  local current

  current="$(env_value "$key")"
  if [ -n "$current" ] && [ "$current" != "$default_value" ]; then
    if [ "$current" != "$value" ]; then
      warn "$key already set; leaving existing value"
    fi
    return
  fi
  if [ "$CHECK_ONLY" -eq 1 ]; then
    if [ "$current" != "$value" ]; then
      warn "$key is ${current:-empty}; would set $value"
    fi
    return
  fi
  set_env_value "$key" "$value"
  log "set $key"
}

set_env_from_env_var() {
  local key="$1"
  local value="${!key-}"

  [ -n "$value" ] || return 0
  if [ "$CHECK_ONLY" -eq 1 ]; then
    if [ "$(env_value "$key")" != "$value" ]; then
      warn "$key would be set to $value"
    fi
    return
  fi
  set_env_value "$key" "$value"
  log "set $key"
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

append_allowed_origin() {
  local domain="$1"
  local current

  current="$(env_value AGENTFORGE_ALLOWED_ORIGINS)"
  case ",${current}," in
    *,"$domain",*) return ;;
  esac
  if [ "$CHECK_ONLY" -eq 1 ]; then
    warn "AGENTFORGE_ALLOWED_ORIGINS does not include $domain"
    return
  fi
  if [ -z "$current" ]; then
    set_env_value AGENTFORGE_ALLOWED_ORIGINS "$domain,localhost,127.0.0.1"
  else
    set_env_value AGENTFORGE_ALLOWED_ORIGINS "$current,$domain"
  fi
  log "added $domain to AGENTFORGE_ALLOWED_ORIGINS"
}

validate_compose() {
  docker compose --env-file "$ENV_FILE" -f "$ROOT_DIR/docker/compose.yml" -f "$ROOT_DIR/docker/compose.prod.yml" --profile prod config -q
}

require_prod_env() {
  local missing=""
  local key

  for key in \
    APP_HOST \
    APP_URL \
    CORS_ORIGIN \
    POSTGRES_PASSWORD \
    REDIS_PASSWORD \
    JWT_SECRET \
    MCP_TOKEN \
    API_KEY_SALT \
    LLM_ENCRYPTION_KEY \
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
      missing="${missing}${key} "
    fi
  done

  if [ -n "$missing" ]; then
    die "missing production env values in $ENV_FILE: $missing. Run make bootstrap-selfhost, then rerun make selfhost-check."
  fi
}

validate_caddy() {
  local output
  output="$(mktemp)"
  if ! docker run --rm \
    -e APP_HOST="$DOMAIN" \
    -v "$ROOT_DIR/docker/caddy/Caddyfile:/etc/caddy/Caddyfile:ro" \
    caddy:2.10-alpine caddy validate --config /etc/caddy/Caddyfile --adapter caddyfile >"$output" 2>&1; then
    cat "$output" >&2
    rm -f "$output"
    die "Caddy configuration is invalid"
  fi
  rm -f "$output"
  log "Caddy configuration is valid"
}

prepare_env_file() {
  if [ -f "$ENV_FILE" ]; then
    return 0
  fi
  if [ "$CHECK_ONLY" -eq 1 ]; then
    WRITE_ALLOWED=0
    warn "$ENV_FILE is missing; run make bootstrap-selfhost without --check"
    return 0
  fi
  [ -f "$ENV_EXAMPLE" ] || die "missing $ENV_EXAMPLE"
  mkdir -p "$(dirname "$ENV_FILE")"
  cp "$ENV_EXAMPLE" "$ENV_FILE"
  chmod 600 "$ENV_FILE"
  CREATED_ENV=1
  log "created $ENV_FILE from $ENV_EXAMPLE"
}

fill_selfhost_env() {
  local need_nats=0
  local key

  [ -f "$ENV_FILE" ] || return 0

  for key in \
    NATS_CALLOUT_ISSUER_SEED \
    NATS_CALLOUT_ACCOUNT_SIGNING_KEY_SEED \
    NATS_CALLOUT_XKEY_SEED \
    NATS_CALLOUT_ISSUER_PUBLIC \
    NATS_CALLOUT_XKEY_PUBLIC
  do
    if [ -z "$(env_value "$key")" ]; then
      need_nats=1
    fi
  done

  if [ "$need_nats" -eq 1 ] && [ "$CHECK_ONLY" -eq 0 ] && [ "$WRITE_ALLOWED" -eq 1 ]; then
    log "generating NATS callout key material"
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
}

require_cmd docker
if ! docker compose version >/dev/null 2>&1; then
  die "Docker Compose v2 is required"
fi

DOMAIN="$(normalize_host "$DOMAIN")"
if [ -z "$DOMAIN" ]; then
  DOMAIN="$(normalize_host "$(env_value APP_HOST)")"
fi
if [ -z "$DOMAIN" ]; then
  DOMAIN="$(normalize_host "$(env_value APP_URL)")"
fi
if [ -z "$DOMAIN" ]; then
  DOMAIN="localhost"
fi

prepare_env_file
fill_selfhost_env

if [ "$CREATED_ENV" -eq 1 ]; then
  log "self-host env file is ready"
fi

if [ -n "$UPDATED_VALUES" ]; then
  log "filled self-host values: $UPDATED_VALUES"
fi

if [ -n "$MISSING_VALUES" ]; then
  warn "missing values: $MISSING_VALUES"
fi

if [ -f "$ENV_FILE" ]; then
  set_env_from_env_var HTTP_PORT
  set_env_from_env_var HTTPS_PORT
  set_env_if_empty_or_default APP_HOST "$DOMAIN" "localhost"
  set_env_if_empty_or_default APP_URL "$(public_url "$DOMAIN")" "https://localhost"
  set_env_if_empty_or_default CORS_ORIGIN "$(public_url "$DOMAIN")" "https://localhost"
  append_allowed_origin "$DOMAIN"
fi

if [ -f "$ENV_FILE" ]; then
  require_prod_env
  validate_compose
  log "prod Compose configuration is valid"
else
  warn "skipping Compose validation because docker/.env is missing"
fi

validate_caddy

cat <<NEXT

Next self-host commands:
  make prod-pull
  make selfhost-health
  make prod-logs

Open:
  $(public_url "$DOMAIN")

Build from source instead of GHCR images:
  make prod

Health checks:
  make selfhost-check

Stop self-host stack:
  make prod-down
NEXT

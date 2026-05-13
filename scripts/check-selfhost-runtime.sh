#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ENV_FILE="${ENV_FILE:-$ROOT_DIR/docker/.env}"
WAIT=0
TIMEOUT_SECONDS=120
DOMAIN=""
INSECURE=0
PUBLIC_INGRESS=0
FAILURES=0
CONTAINER_NAME_PREFIX="${CONTAINER_NAME_PREFIX:-agentforge}"
TEMPORAL_CONTAINER="${TEMPORAL_CONTAINER:-${CONTAINER_NAME_PREFIX}-temporal}"

usage() {
  cat <<'USAGE'
Check the self-contained Wisdoverse Forge production ingress.

Usage:
  scripts/check-selfhost-runtime.sh
  scripts/check-selfhost-runtime.sh --wait
  scripts/check-selfhost-runtime.sh --domain forge.example.com
  scripts/check-selfhost-runtime.sh --domain forge.example.com --public-ingress
  scripts/check-selfhost-runtime.sh --domain localhost --insecure
  HTTPS_PORT=18443 scripts/check-selfhost-runtime.sh --domain localhost --insecure

Checks the public Caddy URL, API liveness, API readiness, and the internal
Temporal container when it exists. Localhost uses --insecure automatically
because Caddy's local CA is usually not trusted by the host browser or curl.
Use --public-ingress on real public domains to verify default :80 redirects to
HTTPS and default :443 presents a publicly trusted TLS certificate.
USAGE
}

log() {
  printf '[selfhost-health] %s\n' "$*"
}

warn() {
  printf '[selfhost-health] WARNING: %s\n' "$*" >&2
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

url_port() {
  local value="$1"
  value="${value#http://}"
  value="${value#https://}"
  value="${value%%/*}"
  value="${value#[}"
  value="${value%]}"
  case "$value" in
    *:*) printf '%s' "${value##*:}" ;;
  esac
}

public_url() {
  local domain="$1"
  local port

  port="$(env_or_file_value HTTPS_PORT)"
  if [ -z "$port" ] || [ "$port" = "443" ]; then
    port="$(url_port "$(env_or_file_value APP_URL)")"
  fi
  if [ -n "$port" ] && [ "$port" != "443" ]; then
    printf 'https://%s:%s' "$domain" "$port"
  else
    printf 'https://%s' "$domain"
  fi
}

is_local_domain() {
  case "$1" in
    localhost|127.0.0.1|::1|*.local) return 0 ;;
    *) return 1 ;;
  esac
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --wait)
      WAIT=1
      ;;
    --timeout)
      shift
      [ "$#" -gt 0 ] || {
        usage >&2
        exit 2
      }
      TIMEOUT_SECONDS="$1"
      ;;
    --domain)
      shift
      [ "$#" -gt 0 ] || {
        usage >&2
        exit 2
      }
      DOMAIN="$(normalize_host "$1")"
      ;;
    --insecure)
      INSECURE=1
      ;;
    --public-ingress)
      PUBLIC_INGRESS=1
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      usage >&2
      printf '[selfhost-health] ERROR: unknown argument: %s\n' "$1" >&2
      exit 2
      ;;
  esac
  shift
done

require_cmd() {
  if ! command -v "$1" >/dev/null 2>&1; then
    printf '[selfhost-health] ERROR: missing required command: %s\n' "$1" >&2
    exit 2
  fi
}

if [ -z "$DOMAIN" ]; then
  DOMAIN="$(normalize_host "$(env_value APP_HOST)")"
fi
if [ -z "$DOMAIN" ]; then
  DOMAIN="$(normalize_host "$(env_value APP_URL)")"
fi
if [ -z "$DOMAIN" ]; then
  DOMAIN="localhost"
fi

case "$DOMAIN" in
  localhost|127.0.0.1|::1|*.local) INSECURE=1 ;;
esac

BASE_URL="${BASE_URL:-$(public_url "$DOMAIN")}"
CURL_ARGS=(-fsS --max-time 8)
if [ "$INSECURE" -eq 1 ]; then
  CURL_ARGS+=(-k)
fi

curl_body() {
  curl "${CURL_ARGS[@]}" "$1"
}

record_pass() {
  printf '[selfhost-health] PASS: %s\n' "$1"
}

record_fail() {
  printf '[selfhost-health] FAIL: %s -- %s\n' "$1" "$2" >&2
  FAILURES=$((FAILURES + 1))
}

run_probe_once() {
  local name="$1"
  local body
  local compact

  case "$name" in
    frontend)
      body="$(curl_body "$BASE_URL" 2>&1)" || return 1
      case "$body" in
        *'<div id="root"'*|*'Wisdoverse Forge'*) return 0 ;;
      esac
      printf '%s' "$body"
      return 1
      ;;
    api-liveness)
      body="$(curl_body "${BASE_URL}/health" 2>&1)" || return 1
      compact="$(printf '%s' "$body" | tr -d '[:space:]')"
      case "$compact" in
        *'"ok":true'*|*'"status":"healthy"'*) return 0 ;;
      esac
      printf '%s' "$body"
      return 1
      ;;
    api-readiness)
      body="$(curl_body "${BASE_URL}/api/health" 2>&1)" || return 1
      compact="$(printf '%s' "$body" | tr -d '[:space:]')"
      case "$compact" in
        *'"status":"ready"'*) return 0 ;;
      esac
      printf '%s' "$body"
      return 1
      ;;
    temporal)
      command -v docker >/dev/null 2>&1 || return 1
      docker inspect "$TEMPORAL_CONTAINER" >/dev/null 2>&1 || return 1
      docker exec "$TEMPORAL_CONTAINER" temporal operator cluster health --address temporal-internal:7233 >/dev/null 2>&1
      ;;
    public-http-redirect)
      if is_local_domain "$DOMAIN"; then
        printf 'public ingress requires a public DNS name'
        return 1
      fi
      local redirect
      redirect="$(curl -sS --max-time 8 -o /dev/null -w '%{http_code} %{redirect_url}' "http://${DOMAIN}/" 2>&1)" || {
        printf '%s' "$redirect"
        return 1
      }
      case "$redirect" in
        301\ https://"${DOMAIN}"/*|302\ https://"${DOMAIN}"/*|308\ https://"${DOMAIN}"/*) return 0 ;;
      esac
      printf '%s' "$redirect"
      return 1
      ;;
    public-https-tls)
      if is_local_domain "$DOMAIN"; then
        printf 'public ingress requires a public DNS name'
        return 1
      fi
      if [ "$INSECURE" -eq 1 ]; then
        printf 'public ingress cannot be verified with --insecure'
        return 1
      fi
      curl -fsSI --max-time 8 "https://${DOMAIN}/" >/dev/null
      ;;
    *)
      return 1
      ;;
  esac
}

wait_for_probe() {
  local name="$1"
  local label="$2"
  local deadline
  local last_error=""

  if [ "$WAIT" -eq 0 ]; then
    if last_error="$(run_probe_once "$name" 2>&1)"; then
      record_pass "$label"
    else
      record_fail "$label" "${last_error:-not ready}"
    fi
    return
  fi

  deadline=$((SECONDS + TIMEOUT_SECONDS))
  while [ "$SECONDS" -le "$deadline" ]; do
    if last_error="$(run_probe_once "$name" 2>&1)"; then
      record_pass "$label"
      return
    fi
    sleep 2
  done

  record_fail "$label" "${last_error:-timed out after ${TIMEOUT_SECONDS}s}"
}

require_cmd curl

log "Public URL: ${BASE_URL}"
if [ "$INSECURE" -eq 1 ]; then
  log "TLS verification disabled for this check"
fi

if [ "$PUBLIC_INGRESS" -eq 1 ]; then
  wait_for_probe public-http-redirect "Public HTTP :80 redirects to HTTPS"
  wait_for_probe public-https-tls "Public HTTPS :443 has trusted TLS"
fi

wait_for_probe frontend "Frontend shell"
wait_for_probe api-liveness "Rust API /health through Caddy"
wait_for_probe api-readiness "Rust API /api/health through Caddy"
wait_for_probe temporal "Temporal cluster health"

if [ "$FAILURES" -gt 0 ]; then
  warn "self-host health check failed with ${FAILURES} failing probe(s)"
  exit 1
fi

log "self-host runtime is reachable"

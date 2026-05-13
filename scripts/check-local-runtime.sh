#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ENV_FILE="$ROOT_DIR/docker/.env"
WAIT=0
TIMEOUT_SECONDS=90
CHECK_FRONTEND=0
FAILURES=0

usage() {
  cat <<'USAGE'
Check the local Wisdoverse Forge runtime.

Usage:
  scripts/check-local-runtime.sh
  scripts/check-local-runtime.sh --wait
  scripts/check-local-runtime.sh --wait --timeout 180
  scripts/check-local-runtime.sh --frontend

Checks API liveness, API readiness, orchestrator health, NATS monitoring health,
and Temporal cluster health. Frontend is optional because Vite runs in a
separate terminal during local development.
USAGE
}

log() {
  printf '[runtime-check] %s\n' "$*"
}

warn() {
  printf '[runtime-check] WARNING: %s\n' "$*" >&2
}

env_or_file() {
  local key="$1"
  local fallback="$2"
  local value="${!key:-}"

  if [ -n "$value" ]; then
    printf '%s' "$value"
    return
  fi
  if [ -f "$ENV_FILE" ]; then
    value="$(sed -n "s/^${key}=//p" "$ENV_FILE" | tail -n 1 | tr -d '\r')"
    if [ -n "$value" ]; then
      printf '%s' "$value"
      return
    fi
  fi
  printf '%s' "$fallback"
}

API_PORT="$(env_or_file AGENTFORGE_PORT 4003)"
ORCHESTRATOR_PORT="$(env_or_file ORCHESTRATOR_PORT 4010)"
NATS_MONITOR_PORT="$(env_or_file NATS_MONITOR_PORT 8222)"
CLIENT_PORT="$(env_or_file AGENTFORGE_CLIENT_PORT 4002)"

API_BASE_URL="${API_BASE_URL:-http://127.0.0.1:${API_PORT}}"
ORCHESTRATOR_BASE_URL="${ORCHESTRATOR_BASE_URL:-http://127.0.0.1:${ORCHESTRATOR_PORT}}"
NATS_MONITOR_URL="${NATS_MONITOR_URL:-http://127.0.0.1:${NATS_MONITOR_PORT}}"
FRONTEND_URL="${FRONTEND_URL:-http://127.0.0.1:${CLIENT_PORT}}"

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
    --frontend)
      CHECK_FRONTEND=1
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      usage >&2
      printf '[runtime-check] ERROR: unknown argument: %s\n' "$1" >&2
      exit 2
      ;;
  esac
  shift
done

require_cmd() {
  if ! command -v "$1" >/dev/null 2>&1; then
    printf '[runtime-check] ERROR: missing required command: %s\n' "$1" >&2
    exit 2
  fi
}

curl_body() {
  curl -fsS --max-time 5 "$1"
}

record_pass() {
  printf '[runtime-check] PASS: %s\n' "$1"
}

record_fail() {
  printf '[runtime-check] FAIL: %s -- %s\n' "$1" "$2" >&2
  FAILURES=$((FAILURES + 1))
}

run_probe_once() {
  local name="$1"
  local body
  local compact

  case "$name" in
    api-liveness)
      body="$(curl_body "${API_BASE_URL}/health" 2>&1)" || return 1
      compact="$(printf '%s' "$body" | tr -d '[:space:]')"
      case "$compact" in
        *'"ok":true'*|*'"status":"healthy"'*) return 0 ;;
      esac
      printf '%s' "$body"
      return 1
      ;;
    api-readiness)
      body="$(curl_body "${API_BASE_URL}/api/health" 2>&1)" || return 1
      compact="$(printf '%s' "$body" | tr -d '[:space:]')"
      case "$compact" in
        *'"status":"ready"'*) return 0 ;;
      esac
      printf '%s' "$body"
      return 1
      ;;
    orchestrator)
      body="$(curl_body "${ORCHESTRATOR_BASE_URL}/health" 2>&1)" || return 1
      compact="$(printf '%s' "$body" | tr -d '[:space:]')"
      case "$compact" in
        *'"status":"healthy"'*|*'"healthy"'*) return 0 ;;
      esac
      printf '%s' "$body"
      return 1
      ;;
    nats)
      body="$(curl_body "${NATS_MONITOR_URL}/healthz" 2>&1)" || return 1
      compact="$(printf '%s' "$body" | tr -d '[:space:]')"
      case "$compact" in
        *'"status":"ok"'*) return 0 ;;
      esac
      printf '%s' "$body"
      return 1
      ;;
    temporal)
      command -v docker >/dev/null 2>&1 || return 1
      docker inspect agentforge-temporal >/dev/null 2>&1 || return 1
      docker exec agentforge-temporal temporal operator cluster health --address temporal-internal:7233 >/dev/null 2>&1
      ;;
    frontend)
      curl_body "$FRONTEND_URL" >/dev/null 2>&1
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

log "API base: ${API_BASE_URL}"
log "Orchestrator base: ${ORCHESTRATOR_BASE_URL}"
log "NATS monitor: ${NATS_MONITOR_URL}"

wait_for_probe api-liveness "Rust API /health"
wait_for_probe api-readiness "Rust API /api/health"
wait_for_probe orchestrator "Rust orchestrator /health"
wait_for_probe nats "NATS /healthz"
wait_for_probe temporal "Temporal cluster health"

if [ "$CHECK_FRONTEND" -eq 1 ]; then
  log "Frontend URL: ${FRONTEND_URL}"
  wait_for_probe frontend "Vite frontend"
else
  log "Frontend check skipped; run npm run dev and open ${FRONTEND_URL}"
fi

if [ "$FAILURES" -gt 0 ]; then
  warn "runtime check failed with ${FAILURES} failing probe(s)"
  exit 1
fi

log "local runtime is healthy"

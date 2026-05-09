#!/bin/sh
set -eu

COMPOSE_FILE="${SLO_COMPOSE_FILE:-docker/compose.yml}"
COMPOSE_PROFILE="${SLO_COMPOSE_PROFILE:-external}"
TARGET_SERVICE="${SLO_TARGET_SERVICE:-agentforge}"
TARGET_PORT="${SLO_TARGET_PORT:-${AGENTFORGE_PORT:-4003}}"

SAMPLE_COUNT="${SLO_SAMPLE_COUNT:-20}"
MIN_SUCCESS_RATE="${SLO_MIN_SUCCESS_RATE:-95}"
MAX_P95_MS="${SLO_MAX_P95_MS:-500}"
MAX_ERROR_BUDGET_PERCENT="${SLO_MAX_ERROR_BUDGET_PERCENT:-5}"
WS_SAMPLE_COUNT="${SLO_WS_SAMPLE_COUNT:-10}"
WS_MIN_SUCCESS_RATE="${SLO_WS_MIN_SUCCESS_RATE:-95}"
MAX_WS_P95_MS="${SLO_MAX_WS_P95_MS:-150}"
WS_CONNECT_TIMEOUT_MS="${SLO_WS_CONNECT_TIMEOUT_MS:-5000}"
STARTUP_TIMEOUT_SEC="${SLO_STARTUP_TIMEOUT_SEC:-120}"
STARTUP_POLL_INTERVAL_SEC="${SLO_STARTUP_POLL_INTERVAL_SEC:-2}"

LOG_PREFIX="[deploy-slo]"
TMP_TIMINGS_FILE="$(mktemp)"
TMP_WS_TIMINGS_FILE="$(mktemp)"

cleanup() {
  rm -f "$TMP_TIMINGS_FILE"
  rm -f "$TMP_WS_TIMINGS_FILE"
}
trap cleanup EXIT INT TERM

log_info() {
  echo "$LOG_PREFIX INFO: $*"
}

log_error() {
  echo "$LOG_PREFIX ERROR: $*" >&2
}

require_positive_int() {
  value="$1"
  name="$2"
  case "$value" in
    ''|*[!0-9]*)
      log_error "$name must be a positive integer (current: $value)"
      exit 1
      ;;
    0)
      log_error "$name must be greater than 0"
      exit 1
      ;;
  esac
}

require_non_negative_int() {
  value="$1"
  name="$2"
  case "$value" in
    ''|*[!0-9]*)
      log_error "$name must be a non-negative integer (current: $value)"
      exit 1
      ;;
  esac
}

compose_exec() {
  cmd="$1"
  docker compose -f "$COMPOSE_FILE" --profile "$COMPOSE_PROFILE" exec -T "$TARGET_SERVICE" sh -lc "$cmd"
}

wait_for_liveness() {
  attempts=$((STARTUP_TIMEOUT_SEC / STARTUP_POLL_INTERVAL_SEC))
  if [ "$attempts" -lt 1 ]; then
    attempts=1
  fi

  i=1
  while [ "$i" -le "$attempts" ]; do
    if compose_exec "curl -fsS http://localhost:${TARGET_PORT}/health/live >/dev/null" >/dev/null 2>&1; then
      log_info "Liveness probe succeeded on attempt $i/$attempts"
      return
    fi
    sleep "$STARTUP_POLL_INTERVAL_SEC"
    i=$((i + 1))
  done

  log_error "Service did not become live within ${STARTUP_TIMEOUT_SEC}s"
  exit 1
}

verify_readiness() {
  readiness_payload="$(compose_exec "curl -fsS http://localhost:${TARGET_PORT}/health/ready" 2>&1 || true)"
  case "$readiness_payload" in
    *'"status":"ready"'*) ;;
    *)
      log_error "Readiness endpoint is not ready: $readiness_payload"
      exit 1
      ;;
  esac

  health_payload="$(compose_exec "curl -fsS http://localhost:${TARGET_PORT}/health" 2>&1 || true)"
  case "$health_payload" in
    *'"status":"unhealthy"'*)
      log_error "Overall health endpoint reports unhealthy: $health_payload"
      exit 1
      ;;
    *) ;;
  esac
}

sample_slo() {
  : > "$TMP_TIMINGS_FILE"
  success_count=0
  i=1

  while [ "$i" -le "$SAMPLE_COUNT" ]; do
    timing="$(compose_exec "curl -fsS -o /dev/null -w '%{time_total}' http://localhost:${TARGET_PORT}/health/ready" 2>/dev/null || true)"
    case "$timing" in
      [0-9]*.[0-9]*)
        printf '%s\n' "$timing" >> "$TMP_TIMINGS_FILE"
        success_count=$((success_count + 1))
        ;;
      *)
        # Non-numeric or empty output — not counted as success
        ;;
    esac
    i=$((i + 1))
  done

  success_rate=$((success_count * 100 / SAMPLE_COUNT))
  if [ "$success_rate" -lt "$MIN_SUCCESS_RATE" ]; then
    log_error "Availability smoke gate failed: successRate=${success_rate}% (< ${MIN_SUCCESS_RATE}%)"
    exit 1
  fi

  error_budget_percent=$((100 - success_rate))
  if [ "$error_budget_percent" -gt "$MAX_ERROR_BUDGET_PERCENT" ]; then
    log_error "Error budget gate failed: consumed=${error_budget_percent}% (> ${MAX_ERROR_BUDGET_PERCENT}%)"
    exit 1
  fi

  sampled_count="$(wc -l < "$TMP_TIMINGS_FILE" | tr -d '[:space:]')"
  if [ "$sampled_count" -eq 0 ]; then
    log_error "No successful latency samples were collected"
    exit 1
  fi

  p95_rank=$(( (95 * sampled_count + 99) / 100 ))
  p95_seconds="$(sort -n "$TMP_TIMINGS_FILE" | sed -n "${p95_rank}p")"
  p95_ms="$(awk -v s="$p95_seconds" 'BEGIN { printf "%.0f", s * 1000 }')"

  if [ "$p95_ms" -gt "$MAX_P95_MS" ]; then
    log_error "Latency smoke gate failed: p95=${p95_ms}ms (> ${MAX_P95_MS}ms)"
    exit 1
  fi

  log_info "HTTP gate passed: successRate=${success_rate}% errorBudget=${error_budget_percent}% p95=${p95_ms}ms samples=${sampled_count}/${SAMPLE_COUNT}"
}

sample_websocket_slo() {
  : > "$TMP_WS_TIMINGS_FILE"
  success_count=0
  i=1

  while [ "$i" -le "$WS_SAMPLE_COUNT" ]; do
    probe_cmd="node --input-type=module -e 'import WebSocket from \"ws\"; const start = Date.now(); const ws = new WebSocket(\"ws://localhost:${TARGET_PORT}/ws\"); const timer = setTimeout(() => { console.error(\"timeout\"); process.exit(1); }, ${WS_CONNECT_TIMEOUT_MS}); ws.once(\"open\", () => { const ms = Date.now() - start; clearTimeout(timer); console.log(ms); ws.close(); setTimeout(() => process.exit(0), 10); }); ws.once(\"error\", (err) => { clearTimeout(timer); console.error(err?.message ?? \"ws-error\"); process.exit(1); });'"
    ws_ms="$(compose_exec "$probe_cmd" 2>/dev/null || true)"
    case "$ws_ms" in
      ''|*[!0-9]*)
        ;;
      *)
        printf '%s\n' "$ws_ms" >> "$TMP_WS_TIMINGS_FILE"
        success_count=$((success_count + 1))
        ;;
    esac
    i=$((i + 1))
  done

  ws_success_rate=$((success_count * 100 / WS_SAMPLE_COUNT))
  if [ "$ws_success_rate" -lt "$WS_MIN_SUCCESS_RATE" ]; then
    log_error "WebSocket availability gate failed: successRate=${ws_success_rate}% (< ${WS_MIN_SUCCESS_RATE}%)"
    exit 1
  fi

  sampled_count="$(wc -l < "$TMP_WS_TIMINGS_FILE" | tr -d '[:space:]')"
  if [ "$sampled_count" -eq 0 ]; then
    log_error "No successful WebSocket latency samples were collected"
    exit 1
  fi

  p95_rank=$(( (95 * sampled_count + 99) / 100 ))
  ws_p95_ms="$(sort -n "$TMP_WS_TIMINGS_FILE" | sed -n "${p95_rank}p")"

  if [ "$ws_p95_ms" -gt "$MAX_WS_P95_MS" ]; then
    log_error "WebSocket latency gate failed: p95=${ws_p95_ms}ms (> ${MAX_WS_P95_MS}ms)"
    exit 1
  fi

  log_info "WebSocket gate passed: successRate=${ws_success_rate}% p95=${ws_p95_ms}ms samples=${sampled_count}/${WS_SAMPLE_COUNT}"
}

main() {
  require_positive_int "$TARGET_PORT" "SLO_TARGET_PORT"
  require_positive_int "$SAMPLE_COUNT" "SLO_SAMPLE_COUNT"
  require_positive_int "$MIN_SUCCESS_RATE" "SLO_MIN_SUCCESS_RATE"
  require_positive_int "$MAX_P95_MS" "SLO_MAX_P95_MS"
  require_non_negative_int "$MAX_ERROR_BUDGET_PERCENT" "SLO_MAX_ERROR_BUDGET_PERCENT"
  require_positive_int "$WS_SAMPLE_COUNT" "SLO_WS_SAMPLE_COUNT"
  require_positive_int "$WS_MIN_SUCCESS_RATE" "SLO_WS_MIN_SUCCESS_RATE"
  require_positive_int "$MAX_WS_P95_MS" "SLO_MAX_WS_P95_MS"
  require_positive_int "$WS_CONNECT_TIMEOUT_MS" "SLO_WS_CONNECT_TIMEOUT_MS"
  require_positive_int "$STARTUP_TIMEOUT_SEC" "SLO_STARTUP_TIMEOUT_SEC"
  require_positive_int "$STARTUP_POLL_INTERVAL_SEC" "SLO_STARTUP_POLL_INTERVAL_SEC"

  log_info "Running deploy smoke gate for ${TARGET_SERVICE} via ${COMPOSE_FILE} profile=${COMPOSE_PROFILE}"
  log_info "HTTP thresholds: samples=${SAMPLE_COUNT}, minSuccess=${MIN_SUCCESS_RATE}%, maxP95=${MAX_P95_MS}ms, maxErrorBudget=${MAX_ERROR_BUDGET_PERCENT}%"
  log_info "WS thresholds: samples=${WS_SAMPLE_COUNT}, minSuccess=${WS_MIN_SUCCESS_RATE}%, maxP95=${MAX_WS_P95_MS}ms"

  wait_for_liveness
  verify_readiness
  sample_slo
  sample_websocket_slo
}

main "$@"

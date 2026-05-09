#!/usr/bin/env sh
set -eu

TARGET_NAME="${TARGET_NAME:-local-prod-ext}"
API_BASE_URL="${API_BASE_URL:-http://127.0.0.1:4003}"
ORCHESTRATOR_BASE_URL="${ORCHESTRATOR_BASE_URL:-http://127.0.0.1:4010}"
NATS_MONITOR_URL="${NATS_MONITOR_URL:-http://127.0.0.1:8222}"
METRICS_URL="${METRICS_URL:-${API_BASE_URL}/metrics}"
CURL_CONNECT_TIMEOUT="${CURL_CONNECT_TIMEOUT:-5}"
CURL_MAX_TIME="${CURL_MAX_TIME:-15}"
SNAPSHOT_OUTPUT="${SNAPSHOT_OUTPUT:-}"
SNAPSHOT_REQUIRE_OPTIONAL="${SNAPSHOT_REQUIRE_OPTIONAL:-false}"
PSQL_DOCKER_NETWORK="${PSQL_DOCKER_NETWORK:-}"
PSQL_DOCKER_IMAGE="${PSQL_DOCKER_IMAGE:-postgres:16-alpine}"
DOCKER_CONTAINER="${DOCKER_CONTAINER:-}"

TMP_MD="$(mktemp)"
TMP_BODY="$(mktemp)"
TMP_METRICS="$(mktemp)"
TMP_NATS="$(mktemp)"
TMP_DB="$(mktemp)"

failures=0
skips=0

cleanup() {
  rm -f "$TMP_MD" "$TMP_BODY" "$TMP_METRICS" "$TMP_NATS" "$TMP_DB"
}
trap cleanup EXIT INT TERM

usage() {
  cat <<'EOF'
Usage: scripts/release/orchestration_gate_snapshot.sh

Collect a point-in-time orchestration release-gate snapshot for staging soak,
canary, or local prod-ext evidence.

Environment:
  TARGET_NAME                 Label printed in the report. Default: local-prod-ext
  API_BASE_URL                Rust API base URL. Default: http://127.0.0.1:4003
  ORCHESTRATOR_BASE_URL       Orchestrator base URL. Default: http://127.0.0.1:4010
  NATS_MONITOR_URL            NATS monitor URL. Default: http://127.0.0.1:8222
  METRICS_URL                 Prometheus endpoint. Default: $API_BASE_URL/metrics
  METRICS_BEARER_TOKEN        Optional bearer token for admin-gated /metrics.
  DATABASE_URL                Optional Postgres DSN used by psql. Never printed.
  PGHOST/PGUSER/PGDATABASE    Optional psql connection env, preferred on shared hosts.
  PSQL_DOCKER_NETWORK         Optional Docker network for running psql in a
                              throwaway container, useful for local prod-ext
                              external DB hosts.
  PSQL_DOCKER_IMAGE           Image for Docker psql mode. Default: postgres:16-alpine
  DOCKER_CONTAINER            Optional local container name for reading only the
                              named ORCHESTRATION_* feature flags.
  SNAPSHOT_OUTPUT             Optional report path. Default: stdout.
  SNAPSHOT_REQUIRE_OPTIONAL   true => fail when optional DB/metrics/NATS are skipped.

The report intentionally does not print DSNs, bearer tokens, JWTs, or raw log
payloads. It is supporting evidence only; it cannot replace 24h soak, canary
timeline, rollback drill, dashboard links, or owner signoff.
EOF
}

case "${1:-}" in
  -h|--help)
    usage
    exit 0
    ;;
  "")
    ;;
  *)
    echo "ERROR: unknown argument: $1" >&2
    usage >&2
    exit 2
    ;;
esac

append() {
  printf '%s\n' "$*" >> "$TMP_MD"
}

one_line() {
  tr '\n\r' '  ' | sed 's/[[:space:]][[:space:]]*/ /g; s/|/\\|/g; s/^ //; s/ $//' | cut -c 1-500
}

disable_xtrace() {
  case "$-" in
    *x*) SNAPSHOT_RESTORE_XTRACE=1; set +x ;;
    *) SNAPSHOT_RESTORE_XTRACE=0 ;;
  esac
}

restore_xtrace() {
  if [ "${SNAPSHOT_RESTORE_XTRACE:-0}" = "1" ]; then
    set -x
  fi
  SNAPSHOT_RESTORE_XTRACE=0
}

metrics_bearer_token_configured() {
  disable_xtrace
  [ -n "${METRICS_BEARER_TOKEN:-}" ]
  status=$?
  restore_xtrace
  return "$status"
}

database_url_configured() {
  disable_xtrace
  [ -n "${DATABASE_URL:-}" ]
  status=$?
  restore_xtrace
  return "$status"
}

pg_env_configured() {
  disable_xtrace
  [ -n "${PGHOST:-}" ] && [ -n "${PGUSER:-}" ] && [ -n "${PGDATABASE:-}" ]
  status=$?
  restore_xtrace
  return "$status"
}

curl_capture() {
  url="$1"
  shift
  curl -fsS --connect-timeout "$CURL_CONNECT_TIMEOUT" --max-time "$CURL_MAX_TIME" "$@" "$url"
}

curl_capture_bearer() {
  url="$1"
  disable_xtrace
  set +e
  curl -fsS --connect-timeout "$CURL_CONNECT_TIMEOUT" --max-time "$CURL_MAX_TIME" --config - "$url" <<EOF
header = "Authorization: Bearer ${METRICS_BEARER_TOKEN}"
EOF
  status=$?
  set -e
  restore_xtrace
  return "$status"
}

status_row() {
  area="$1"
  name="$2"
  status="$3"
  evidence="$4"

  case "$status" in
    PASS) ;;
    FAIL) failures=$((failures + 1)) ;;
    SKIP) skips=$((skips + 1)) ;;
    *) failures=$((failures + 1)) ;;
  esac

  printf '| %s | %s | %s | %s |\n' "$area" "$name" "$status" "$evidence" >> "$TMP_MD"
}

is_zero() {
  value="$1"
  awk -v v="$value" 'BEGIN { exit ((v + 0) == 0 ? 0 : 1) }'
}

metric_value() {
  metric="$1"
  awk -v metric="$metric" '
    $1 == metric { print $2; found=1; exit }
    index($1, metric "{") == 1 { print $2; found=1; exit }
    END { if (!found) exit 1 }
  ' "$TMP_METRICS"
}

collect_health() {
  append "## Snapshot Checks"
  append ""
  append "| Area | Check | Status | Evidence |"
  append "| ---- | ----- | ------ | -------- |"

  if curl_capture "${API_BASE_URL}/api/health" > "$TMP_BODY" 2>/dev/null; then
    body="$(one_line < "$TMP_BODY")"
    case "$body" in
      *'"status":"ready"'*|*'"status": "ready"'*) status_row "health" "rust api readiness" "PASS" "$body" ;;
      *) status_row "health" "rust api readiness" "FAIL" "$body" ;;
    esac
  else
    status_row "health" "rust api readiness" "FAIL" "GET ${API_BASE_URL}/api/health failed"
  fi

  if curl_capture "${ORCHESTRATOR_BASE_URL}/health" > "$TMP_BODY" 2>/dev/null; then
    body="$(one_line < "$TMP_BODY")"
    case "$body" in
      *'"healthy"'*|*'"status":"ok"'*|*'"status": "ok"'*) status_row "health" "orchestrator health" "PASS" "$body" ;;
      *) status_row "health" "orchestrator health" "FAIL" "$body" ;;
    esac
  else
    status_row "health" "orchestrator health" "FAIL" "GET ${ORCHESTRATOR_BASE_URL}/health failed"
  fi

  if curl_capture "${NATS_MONITOR_URL}/healthz" > "$TMP_BODY" 2>/dev/null; then
    body="$(one_line < "$TMP_BODY")"
    case "$body" in
      *'"ok"'*|*'"status":"ok"'*|*'"status": "ok"'*) status_row "health" "nats healthz" "PASS" "$body" ;;
      *) status_row "health" "nats healthz" "FAIL" "$body" ;;
    esac
  else
    status_row "health" "nats healthz" "FAIL" "GET ${NATS_MONITOR_URL}/healthz failed"
  fi
}

collect_feature_flags() {
  for flag in \
    ORCHESTRATION_RESULT_CONSUMER_ENABLED \
    ORCHESTRATION_ASSIGNMENT_OUTBOX_PUBLISHER_ENABLED \
    ORCHESTRATION_PARTICIPANT_LIVENESS_ENABLED \
    ORCHESTRATION_CONTROL_PLANE_METRICS_ENABLED \
    ORCHESTRATION_WS_PROJECTOR_ENABLED
  do
    value="$(eval "printf '%s' \"\${$flag-}\"")"
    if [ -z "$value" ] && [ -n "$DOCKER_CONTAINER" ] && command -v docker >/dev/null 2>&1; then
      value="$(
        docker inspect -f '{{range .Config.Env}}{{println .}}{{end}}' "$DOCKER_CONTAINER" 2>/dev/null \
          | sed -n "s/^${flag}=//p" \
          | head -n 1
      )"
    fi
    if [ -z "$value" ]; then
      status_row "feature-flags" "$flag" "SKIP" "not exported to snapshot environment; verify deployment environment"
      continue
    fi

    normalized="$(printf '%s' "$value" | tr '[:upper:]' '[:lower:]')"
    case "$normalized" in
      true|1|yes|on) status_row "feature-flags" "$flag" "PASS" "$normalized" ;;
      false|0|no|off) status_row "feature-flags" "$flag" "FAIL" "$normalized" ;;
      *) status_row "feature-flags" "$flag" "FAIL" "invalid boolean: $(printf '%s' "$value" | one_line)" ;;
    esac
  done
}

collect_metrics() {
  if metrics_bearer_token_configured; then
    if ! curl_capture_bearer "$METRICS_URL" > "$TMP_METRICS" 2>/dev/null; then
      status_row "metrics" "prometheus scrape" "SKIP" "METRICS_URL unavailable or unauthorized"
      return
    fi
  elif ! curl_capture "$METRICS_URL" > "$TMP_METRICS" 2>/dev/null; then
    status_row "metrics" "prometheus scrape" "SKIP" "METRICS_URL unavailable or unauthorized; set METRICS_BEARER_TOKEN for admin-gated /metrics"
    return
  fi

  status_row "metrics" "prometheus scrape" "PASS" "$METRICS_URL returned Prometheus text"

  for metric in \
    agentforge_orchestration_outbox_backlog \
    agentforge_orchestration_expired_working_leases \
    agentforge_orchestration_busy_participants_without_work \
    agentforge_orchestration_working_tasks_without_busy_participant \
    agentforge_orchestration_stale_participants
  do
    value="$(metric_value "$metric" 2>/dev/null || true)"
    if [ -z "$value" ]; then
      status_row "metrics" "$metric" "FAIL" "metric missing"
    elif is_zero "$value"; then
      status_row "metrics" "$metric" "PASS" "$value"
    else
      status_row "metrics" "$metric" "FAIL" "$value"
    fi
  done

  for metric in \
    agentforge_orchestration_outbox_oldest_age_seconds \
    agentforge_orchestration_control_plane_metrics_errors_total \
    agentforge_orchestration_result_unauthorized_total \
    agentforge_orchestration_result_transient_errors_total \
    agentforge_orchestration_outbox_publish_errors_total
  do
    lines="$(awk -v metric="$metric" '$1 == metric || index($1, metric "{") == 1 { print $0 }' "$TMP_METRICS" | one_line)"
    if [ -n "$lines" ]; then
      status_row "metrics" "$metric" "PASS" "$lines"
    else
      status_row "metrics" "$metric" "SKIP" "metric not emitted in this scrape"
    fi
  done
}

collect_db() {
  if [ -n "$PSQL_DOCKER_NETWORK" ] && ! database_url_configured && ! pg_env_configured; then
    status_row "database" "postgres snapshot" "SKIP" "PSQL_DOCKER_NETWORK requires DATABASE_URL or PGHOST/PGUSER/PGDATABASE"
    return
  fi

  if [ -z "$PSQL_DOCKER_NETWORK" ] && ! database_url_configured && ! pg_env_configured; then
    status_row "database" "postgres snapshot" "SKIP" "DATABASE_URL or PGHOST/PGUSER/PGDATABASE not set; DSN is intentionally never read from repo files"
    return
  fi

  if [ -n "$PSQL_DOCKER_NETWORK" ]; then
    if ! command -v docker >/dev/null 2>&1; then
      status_row "database" "postgres snapshot" "SKIP" "docker not found for PSQL_DOCKER_NETWORK mode"
      return
    fi
  elif ! command -v psql >/dev/null 2>&1; then
    status_row "database" "postgres snapshot" "SKIP" "psql not found"
    return
  fi

  if ! run_psql > "$TMP_DB" 2>/dev/null <<'SQL'
SELECT 'working_tasks', COUNT(*)::text
FROM orchestration_tasks
WHERE status = 'working'
UNION ALL
SELECT 'expired_working_leases', COUNT(*)::text
FROM orchestration_tasks
WHERE status = 'working'
  AND lease_expires_at IS NOT NULL
  AND lease_expires_at < NOW()
UNION ALL
SELECT 'busy_participants_without_work', COUNT(*)::text
FROM participants p
WHERE p.status = 'busy'
  AND NOT EXISTS (
    SELECT 1
    FROM orchestration_tasks t
    WHERE t.organization_id = p.organization_id
      AND t.assigned_agent_id = p.agent_id
      AND t.status = 'working'
  )
UNION ALL
SELECT 'working_tasks_without_busy_participant', COUNT(*)::text
FROM orchestration_tasks t
WHERE t.status = 'working'
  AND NOT EXISTS (
    SELECT 1
    FROM participants p
    WHERE p.organization_id = t.organization_id
      AND p.agent_id = t.assigned_agent_id
      AND p.status = 'busy'
  )
UNION ALL
SELECT 'unpublished_assignment_outbox', COUNT(*)::text
FROM orchestration_outbox
WHERE published_at IS NULL
  AND event_type = 'assignment';
SQL
  then
    status_row "database" "postgres snapshot" "FAIL" "psql query failed; connection details suppressed"
    return
  fi

  while IFS='|' read -r name value; do
    [ -n "$name" ] || continue
    if is_zero "$value"; then
      status_row "database" "$name" "PASS" "$value"
    else
      status_row "database" "$name" "FAIL" "$value"
    fi
  done < "$TMP_DB"
}

run_psql() {
  if [ -n "$PSQL_DOCKER_NETWORK" ]; then
    if database_url_configured; then
      run_psql_docker_database_url
    else
      docker run --rm -i --network "$PSQL_DOCKER_NETWORK" \
        --env PGHOST --env PGPORT --env PGDATABASE --env PGUSER --env PGPASSWORD --env PGSSLMODE \
        "$PSQL_DOCKER_IMAGE" psql -X -v ON_ERROR_STOP=1 -A -t -F '|'
    fi
  elif database_url_configured; then
    run_psql_database_url
  else
    psql -X -v ON_ERROR_STOP=1 -A -t -F '|'
  fi
}

run_psql_database_url() {
  disable_xtrace
  if ! prepare_database_uri_without_password; then
    set +e
    false
    status=$?
    set -e
    restore_xtrace
    return "$status"
  fi
  set +e
  if [ -n "$SNAPSHOT_DB_PASSWORD" ]; then
    PGPASSWORD="$SNAPSHOT_DB_PASSWORD" psql -X -v ON_ERROR_STOP=1 "$SNAPSHOT_DB_URI" -A -t -F '|'
  else
    psql -X -v ON_ERROR_STOP=1 "$SNAPSHOT_DB_URI" -A -t -F '|'
  fi
  status=$?
  set -e
  unset SNAPSHOT_DB_URI SNAPSHOT_DB_PASSWORD
  restore_xtrace
  return "$status"
}

run_psql_docker_database_url() {
  disable_xtrace
  if ! prepare_database_uri_without_password; then
    set +e
    false
    status=$?
    set -e
    restore_xtrace
    return "$status"
  fi
  set +e
  if [ -n "$SNAPSHOT_DB_PASSWORD" ]; then
    PGPASSWORD="$SNAPSHOT_DB_PASSWORD" docker run --rm -i --network "$PSQL_DOCKER_NETWORK" \
      --env PGPASSWORD "$PSQL_DOCKER_IMAGE" psql -X -v ON_ERROR_STOP=1 "$SNAPSHOT_DB_URI" -A -t -F '|'
  else
    docker run --rm -i --network "$PSQL_DOCKER_NETWORK" \
      "$PSQL_DOCKER_IMAGE" psql -X -v ON_ERROR_STOP=1 "$SNAPSHOT_DB_URI" -A -t -F '|'
  fi
  status=$?
  set -e
  unset SNAPSHOT_DB_URI SNAPSHOT_DB_PASSWORD
  restore_xtrace
  return "$status"
}

prepare_database_uri_without_password() {
  case "$DATABASE_URL" in
    postgres://*|postgresql://*) ;;
    *) return 1 ;;
  esac

  db_scheme="${DATABASE_URL%%://*}"
  db_rest="${DATABASE_URL#*://}"
  SNAPSHOT_DB_PASSWORD=""
  case "$db_rest" in
    *\?*)
      db_query="${db_rest#*\?}"
      if query_contains_password_param "$db_query"; then
        return 1
      fi
      ;;
  esac
  case "$db_rest" in
    *@*)
      db_userinfo="${db_rest%%@*}"
      db_hostpath="${db_rest#*@}"
      case "$db_userinfo" in
        *:*)
          db_user="${db_userinfo%%:*}"
          if ! SNAPSHOT_DB_PASSWORD="$(percent_decode_userinfo "${db_userinfo#*:}")"; then
            return 1
          fi
          SNAPSHOT_DB_URI="${db_scheme}://${db_user}@${db_hostpath}"
          ;;
        *)
          SNAPSHOT_DB_URI="$DATABASE_URL"
          ;;
      esac
      ;;
    *)
      SNAPSHOT_DB_URI="$DATABASE_URL"
      ;;
  esac
  return 0
}

query_contains_password_param() {
  query="$1"
  case "$query" in
    *";"*) return 0 ;;
  esac
  while [ -n "$query" ]; do
    case "$query" in
      *"&"*)
        param="${query%%&*}"
        query="${query#*&}"
        ;;
      *)
        param="$query"
        query=""
        ;;
    esac

    key="${param%%=*}"
    if ! key="$(percent_decode_userinfo "$key")"; then
      return 0
    fi
    key="$(printf "%s" "$key" | tr '[:upper:]' '[:lower:]')"
    if [ "$key" = "password" ]; then
      return 0
    fi
  done
  return 1
}

percent_decode_userinfo() {
  printf "%s" "$1" | awk '
    function hex_value(c) {
      if (c >= "0" && c <= "9") {
        return c + 0
      }
      c = toupper(c)
      if (c >= "A" && c <= "F") {
        return index("ABCDEF", c) + 9
      }
      return -1
    }
    BEGIN {
      input = ""
    }
    {
      input = input $0
    }
    END {
      for (i = 1; i <= length(input); i++) {
        c = substr(input, i, 1)
        if (c == "%") {
          encoded = substr(input, i + 1, 2)
          if (length(encoded) != 2 || encoded !~ /^[0-9A-Fa-f][0-9A-Fa-f]$/) {
            exit 1
          }
          hi = hex_value(substr(encoded, 1, 1))
          lo = hex_value(substr(encoded, 2, 1))
          printf "%c", hi * 16 + lo
          i += 2
        } else {
          printf "%s", c
        }
      }
    }
  '
}

collect_nats() {
  if ! command -v jq >/dev/null 2>&1; then
    status_row "nats" "jetstream snapshot" "SKIP" "jq not found"
    return
  fi

  if ! curl_capture "${NATS_MONITOR_URL}/jsz?accounts=true&streams=true&consumers=true&config=true" > "$TMP_NATS" 2>/dev/null; then
    status_row "nats" "jetstream snapshot" "SKIP" "NATS jsz endpoint unavailable"
    return
  fi

  streams="$(
    jq -r '
      .account_details[]?.stream_detail[]?
      | select(.name == "ORCHESTRATION_ASSIGNMENTS" or .name == "ORCHESTRATION_RESULTS")
      | [.name, (.state.messages // .messages // 0), (.consumer_count // (.consumer_detail // [] | length) // 0)]
      | @tsv
    ' "$TMP_NATS"
  )"

  if [ -z "$streams" ]; then
    status_row "nats" "orchestration streams" "FAIL" "ORCHESTRATION_ASSIGNMENTS / ORCHESTRATION_RESULTS not found"
    return
  fi

  printf '%s\n' "$streams" > "$TMP_BODY"
  while IFS="$(printf '\t')" read -r stream messages consumers; do
    evidence="messages=${messages} consumers=${consumers}"
    if is_zero "$messages"; then
      status_row "nats" "$stream" "PASS" "$evidence"
    else
      status_row "nats" "$stream" "FAIL" "$evidence"
    fi
  done < "$TMP_BODY"

  consumers="$(
    jq -r '
      .account_details[]?.stream_detail[]?
      | select(.name == "ORCHESTRATION_RESULTS")
      | .consumer_detail[]?
      | [.name, (.num_ack_pending // 0), (.num_pending // 0)]
      | @tsv
    ' "$TMP_NATS"
  )"

  if [ -n "$consumers" ]; then
    printf '%s\n' "$consumers" > "$TMP_BODY"
    while IFS="$(printf '\t')" read -r consumer ack_pending pending; do
      evidence="ack_pending=${ack_pending} pending=${pending}"
      if is_zero "$ack_pending" && is_zero "$pending"; then
        status_row "nats" "consumer ${consumer}" "PASS" "$evidence"
      else
        status_row "nats" "consumer ${consumer}" "FAIL" "$evidence"
      fi
    done < "$TMP_BODY"
  else
    status_row "nats" "result consumers" "SKIP" "no ORCHESTRATION_RESULTS consumer detail in jsz"
  fi
}

write_header() {
  captured_at="$(date -u '+%Y-%m-%dT%H:%M:%SZ')"
  repo_sha="$(git rev-parse HEAD 2>/dev/null || printf 'unknown')"

  append "# Orchestration Release Gate Snapshot"
  append ""
  append "- target: ${TARGET_NAME}"
  append "- captured_at_utc: ${captured_at}"
  append "- repo_sha: ${repo_sha}"
  append "- api_base_url: ${API_BASE_URL}"
  append "- orchestrator_base_url: ${ORCHESTRATOR_BASE_URL}"
  append "- nats_monitor_url: ${NATS_MONITOR_URL}"
  append "- collector: scripts/release/orchestration_gate_snapshot.sh"
  append "- secret_policy: DSNs, bearer tokens, JWTs, and raw logs are not printed"
  append ""
}

write_footer() {
  append ""
  append "## Non-Automatable Gate Items"
  append ""
  append "This snapshot is supporting evidence only. It does not prove or replace:"
  append ""
  append "- 24h staging soak duration and trend review"
  append "- production canary timeline at 5%, 25%, and 100%"
  append "- rollback drill with feature flags"
  append "- dashboard links or screenshots owned by SRE"
  append "- alert routing confirmation to the on-call target"
  append "- websocket/polling fallback manual browser verification"
  append "- Backend, SRE, Security, and Frontend owner signoffs"
  append ""
  append "## Summary"
  append ""
  append "- failures: ${failures}"
  append "- skipped_optional_checks: ${skips}"
}

main() {
  write_header
  collect_health
  collect_feature_flags
  collect_metrics
  collect_db
  collect_nats
  write_footer

  if [ -n "$SNAPSHOT_OUTPUT" ]; then
    mkdir -p "$(dirname "$SNAPSHOT_OUTPUT")"
    cp "$TMP_MD" "$SNAPSHOT_OUTPUT"
    echo "Wrote orchestration release-gate snapshot: $SNAPSHOT_OUTPUT"
  else
    cat "$TMP_MD"
  fi

  if [ "$failures" -gt 0 ]; then
    exit 1
  fi

  case "$SNAPSHOT_REQUIRE_OPTIONAL" in
    true|1|yes|on)
      if [ "$skips" -gt 0 ]; then
        exit 1
      fi
      ;;
  esac
}

main

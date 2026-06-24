#!/usr/bin/env sh
set -eu

TARGET_NAME="${TARGET_NAME:-staging}"
PROMETHEUS_URL="${PROMETHEUS_URL:-}"
PROMETHEUS_RULES_JSON_FILE="${PROMETHEUS_RULES_JSON_FILE:-}"
PROMETHEUS_ALERTMANAGERS_JSON_FILE="${PROMETHEUS_ALERTMANAGERS_JSON_FILE:-}"
PROMETHEUS_BEARER_TOKEN="${PROMETHEUS_BEARER_TOKEN:-}"
ALERTMANAGER_URL="${ALERTMANAGER_URL:-}"
ALERTMANAGER_READY_TEXT_FILE="${ALERTMANAGER_READY_TEXT_FILE:-}"
ALERT_ROUTE_JSON_FILE="${ALERT_ROUTE_JSON_FILE:-}"
ALERT_ROUTE_EXPECTED_RECEIVER="${ALERT_ROUTE_EXPECTED_RECEIVER:-}"
ALERT_ROUTE_OUTPUT="${ALERT_ROUTE_OUTPUT:-}"
CURL_CONNECT_TIMEOUT="${CURL_CONNECT_TIMEOUT:-5}"
CURL_MAX_TIME="${CURL_MAX_TIME:-15}"

TMP_MD="$(mktemp)"
TMP_RULES="$(mktemp)"
TMP_ALERTMANAGERS="$(mktemp)"
TMP_STATUS="$(mktemp)"
TMP_BODY="$(mktemp)"

failures=0
incomplete=0
skips=0

cleanup() {
  rm -f "$TMP_MD" "$TMP_RULES" "$TMP_ALERTMANAGERS" "$TMP_STATUS" "$TMP_BODY"
}
trap cleanup EXIT INT TERM

usage() {
  cat <<'EOF'
Usage: scripts/release/orchestration_alert_route_check.sh

Collect release-gate evidence that orchestration alert rules are loaded and
that alert routing is configured for the on-call receiver contract.

Environment:
  TARGET_NAME                    Evidence label. Default: staging
  PROMETHEUS_URL                 Prometheus base URL for live /api/v1/rules.
  PROMETHEUS_RULES_JSON_FILE     Offline Prometheus /api/v1/rules fixture.
  PROMETHEUS_ALERTMANAGERS_JSON_FILE
                                 Offline Prometheus /api/v1/alertmanagers fixture.
  PROMETHEUS_BEARER_TOKEN        Optional bearer token for Prometheus.
  ALERTMANAGER_URL               Optional Alertmanager base URL for /-/ready.
  ALERTMANAGER_READY_TEXT_FILE   Offline Alertmanager /-/ready body fixture.
  ALERT_ROUTE_JSON_FILE          Sanitized JSON route-contract file. Expected:
                                 {"routes":[{"receiver":"platform-oncall",
                                 "matchers":["component=\"orchestration\""]}],
                                 "receivers":[{"name":"platform-oncall",
                                 "integration_count":1}]}
  ALERT_ROUTE_EXPECTED_RECEIVER  Expected on-call receiver name or team alias.
  ALERT_ROUTE_OUTPUT             Optional report path. Default: stdout.

The report never prints bearer tokens, webhook URLs, raw Alertmanager config, or
notification secrets. The script checks Prometheus /api/v1/alertmanagers but
does not print discovered target URLs. It does not fetch Alertmanager
/api/v2/status because that API can include the loaded config. A passing report
proves the rules/route contract and Prometheus target wiring only; it does not
replace SRE confirmation that an on-call notification was received.
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

base_url() {
  printf '%s' "$1" | sed 's:/*$::'
}

curl_capture() {
  url="$1"
  shift
  curl -fsS --connect-timeout "$CURL_CONNECT_TIMEOUT" --max-time "$CURL_MAX_TIME" "$@" "$url"
}

curl_capture_with_bearer() {
  url="$1"
  token="$2"
  printf 'header = "Authorization: Bearer %s"\n' "$token" \
    | curl -fsS --connect-timeout "$CURL_CONNECT_TIMEOUT" --max-time "$CURL_MAX_TIME" --config - "$url"
}

status_row() {
  area="$1"
  name="$2"
  status="$3"
  evidence="$4"

  case "$status" in
    PASS) ;;
    FAIL) failures=$((failures + 1)) ;;
    INCOMPLETE) incomplete=$((incomplete + 1)) ;;
    SKIP) skips=$((skips + 1)) ;;
    *) failures=$((failures + 1)) ;;
  esac

  printf '| %s | %s | %s | %s |\n' "$area" "$name" "$status" "$evidence" >> "$TMP_MD"
}

expected_severity() {
  case "$1" in
    OrchestrationAssignmentOutboxBacklog) printf 'warning' ;;
    OrchestrationAssignmentOutboxStalled) printf 'critical' ;;
    OrchestrationExpiredWorkingLeases) printf 'critical' ;;
    OrchestrationStaleParticipants) printf 'warning' ;;
    OrchestrationBusyParticipantWithoutWork) printf 'critical' ;;
    OrchestrationWorkingTaskWithoutBusyParticipant) printf 'critical' ;;
    OrchestrationResultApplyLag) printf 'critical' ;;
    OrchestrationResultUnauthorizedSpike) printf 'warning' ;;
    OrchestrationOutboxPublishErrors) printf 'critical' ;;
    *) printf 'unknown' ;;
  esac
}

expected_metric() {
  case "$1" in
    OrchestrationAssignmentOutboxBacklog) printf 'agentforge_orchestration_outbox_backlog' ;;
    OrchestrationAssignmentOutboxStalled) printf 'agentforge_orchestration_outbox_oldest_age_seconds' ;;
    OrchestrationExpiredWorkingLeases) printf 'agentforge_orchestration_expired_working_leases' ;;
    OrchestrationStaleParticipants) printf 'agentforge_orchestration_stale_participants' ;;
    OrchestrationBusyParticipantWithoutWork) printf 'agentforge_orchestration_busy_participants_without_work' ;;
    OrchestrationWorkingTaskWithoutBusyParticipant) printf 'agentforge_orchestration_working_tasks_without_busy_participant' ;;
    OrchestrationResultApplyLag) printf 'agentforge_orchestration_result_apply_seconds_bucket' ;;
    OrchestrationResultUnauthorizedSpike) printf 'agentforge_orchestration_result_unauthorized_total' ;;
    OrchestrationOutboxPublishErrors) printf 'agentforge_orchestration_outbox_publish_errors_total' ;;
    *) printf 'unknown' ;;
  esac
}

rule_field() {
  alert_name="$1"
  jq_expr="$2"
  jq -r --arg alert "$alert_name" "
    first(
      .data.groups[]? as \$group
      | \$group.rules[]?
      | select((.name // \"\") == \$alert)
      | ${jq_expr}
    ) // \"missing\"
  " "$TMP_RULES" 2>/dev/null | head -n 1
}

copy_or_fetch_prometheus_rules() {
  if [ -n "$PROMETHEUS_RULES_JSON_FILE" ]; then
    if [ ! -r "$PROMETHEUS_RULES_JSON_FILE" ]; then
      status_row "prometheus" "rules source" "FAIL" "PROMETHEUS_RULES_JSON_FILE is not readable"
      return
    fi
    cp "$PROMETHEUS_RULES_JSON_FILE" "$TMP_RULES"
    status_row "prometheus" "rules source" "PASS" "loaded from local fixture"
    return
  fi

  if [ -z "$PROMETHEUS_URL" ]; then
    status_row "prometheus" "rules source" "FAIL" "PROMETHEUS_URL or PROMETHEUS_RULES_JSON_FILE is required"
    return
  fi

  url="$(base_url "$PROMETHEUS_URL")/api/v1/rules?type=alert"
  if [ -n "$PROMETHEUS_BEARER_TOKEN" ]; then
    if curl_capture_with_bearer "$url" "$PROMETHEUS_BEARER_TOKEN" > "$TMP_RULES" 2>/dev/null; then
      status_row "prometheus" "rules source" "PASS" "queried Prometheus /api/v1/rules"
    else
      status_row "prometheus" "rules source" "FAIL" "Prometheus rules query failed"
    fi
  elif curl_capture "$url" > "$TMP_RULES" 2>/dev/null; then
    status_row "prometheus" "rules source" "PASS" "queried Prometheus /api/v1/rules"
  else
    status_row "prometheus" "rules source" "FAIL" "Prometheus rules query failed"
  fi
}

check_prometheus_rules() {
  copy_or_fetch_prometheus_rules
  if [ ! -s "$TMP_RULES" ]; then
    return
  fi
  if ! command -v jq >/dev/null 2>&1; then
    status_row "prometheus" "jq available" "FAIL" "jq is required to inspect Prometheus rules JSON"
    return
  fi

  api_status="$(jq -r '.status // "unknown"' "$TMP_RULES" 2>/dev/null || printf 'invalid')"
  if [ "$api_status" != "success" ]; then
    status_row "prometheus" "rules api status" "FAIL" "status=${api_status}"
    return
  fi
  status_row "prometheus" "rules api status" "PASS" "status=success"

  for alert_name in \
    OrchestrationAssignmentOutboxBacklog \
    OrchestrationAssignmentOutboxStalled \
    OrchestrationExpiredWorkingLeases \
    OrchestrationStaleParticipants \
    OrchestrationBusyParticipantWithoutWork \
    OrchestrationWorkingTaskWithoutBusyParticipant \
    OrchestrationResultApplyLag \
    OrchestrationResultUnauthorizedSpike \
    OrchestrationOutboxPublishErrors
  do
    line="$(
      jq -r --arg alert "$alert_name" '
        .data.groups[]? as $group
        | $group.rules[]?
        | select((.name // "") == $alert)
        | [(.name // ""), (.health // "unknown"), ($group.name // "unknown")]
        | @tsv
      ' "$TMP_RULES" | head -n 1
    )"

    if [ -z "$line" ]; then
      status_row "prometheus" "$alert_name loaded" "FAIL" "alert rule missing from Prometheus"
      continue
    fi

    old_ifs="$IFS"
    IFS="$(printf '\t')"
    read -r _name health group_name <<EOF_RULE
$line
EOF_RULE
    IFS="$old_ifs"

    case "$health" in
      ok)
        status_row "prometheus" "$alert_name loaded" "PASS" "group=${group_name} health=${health}"
        ;;
      unknown)
        status_row "prometheus" "$alert_name loaded" "INCOMPLETE" "group=${group_name} health=${health}; Prometheus did not report rule health"
        ;;
      *)
        status_row "prometheus" "$alert_name loaded" "FAIL" "group=${group_name} health=${health}"
        ;;
    esac

    component="$(rule_field "$alert_name" '.labels.component')"
    if [ "$component" = "orchestration" ]; then
      status_row "prometheus" "$alert_name component label" "PASS" "component=orchestration"
    else
      status_row "prometheus" "$alert_name component label" "FAIL" "component=${component}"
    fi

    expected="$(expected_severity "$alert_name")"
    severity="$(rule_field "$alert_name" '.labels.severity')"
    if [ "$severity" = "$expected" ]; then
      status_row "prometheus" "$alert_name severity" "PASS" "severity=${severity}"
    else
      status_row "prometheus" "$alert_name severity" "FAIL" "expected=${expected} actual=${severity}"
    fi

    runbook="$(rule_field "$alert_name" '.annotations.runbook')"
    case "$runbook" in
      docs/runbooks/orchestration.md#*) status_row "prometheus" "$alert_name runbook annotation" "PASS" "$runbook" ;;
      *) status_row "prometheus" "$alert_name runbook annotation" "FAIL" "runbook=${runbook}" ;;
    esac

    metric="$(expected_metric "$alert_name")"
    query="$(rule_field "$alert_name" '.query // .expr')"
    case "$query" in
      *"$metric"*) status_row "prometheus" "$alert_name expression metric" "PASS" "$metric" ;;
      *) status_row "prometheus" "$alert_name expression metric" "FAIL" "expected metric not found: ${metric}" ;;
    esac

    duration="$(rule_field "$alert_name" '.duration // .for')"
    case "$duration" in
      missing|unknown|"") status_row "prometheus" "$alert_name duration" "INCOMPLETE" "Prometheus did not report rule duration" ;;
      0|0s) status_row "prometheus" "$alert_name duration" "FAIL" "duration=${duration}" ;;
      *) status_row "prometheus" "$alert_name duration" "PASS" "duration=${duration}" ;;
    esac
  done
}

copy_or_fetch_prometheus_alertmanagers() {
  if [ -n "$PROMETHEUS_ALERTMANAGERS_JSON_FILE" ]; then
    if [ ! -r "$PROMETHEUS_ALERTMANAGERS_JSON_FILE" ]; then
      status_row "prometheus" "alertmanager discovery source" "FAIL" "PROMETHEUS_ALERTMANAGERS_JSON_FILE is not readable"
      return
    fi
    cp "$PROMETHEUS_ALERTMANAGERS_JSON_FILE" "$TMP_ALERTMANAGERS"
    status_row "prometheus" "alertmanager discovery source" "PASS" "loaded from local fixture"
    return
  fi

  if [ -z "$PROMETHEUS_URL" ]; then
    status_row "prometheus" "alertmanager discovery source" "INCOMPLETE" "PROMETHEUS_URL or PROMETHEUS_ALERTMANAGERS_JSON_FILE is required"
    return
  fi

  url="$(base_url "$PROMETHEUS_URL")/api/v1/alertmanagers"
  if [ -n "$PROMETHEUS_BEARER_TOKEN" ]; then
    if curl_capture_with_bearer "$url" "$PROMETHEUS_BEARER_TOKEN" > "$TMP_ALERTMANAGERS" 2>/dev/null; then
      status_row "prometheus" "alertmanager discovery source" "PASS" "queried Prometheus /api/v1/alertmanagers"
    else
      status_row "prometheus" "alertmanager discovery source" "FAIL" "Prometheus alertmanager discovery query failed"
    fi
  elif curl_capture "$url" > "$TMP_ALERTMANAGERS" 2>/dev/null; then
    status_row "prometheus" "alertmanager discovery source" "PASS" "queried Prometheus /api/v1/alertmanagers"
  else
    status_row "prometheus" "alertmanager discovery source" "FAIL" "Prometheus alertmanager discovery query failed"
  fi
}

check_prometheus_alertmanager_targets() {
  copy_or_fetch_prometheus_alertmanagers
  if [ ! -s "$TMP_ALERTMANAGERS" ]; then
    return
  fi
  if ! command -v jq >/dev/null 2>&1; then
    status_row "prometheus" "alertmanager discovery jq available" "FAIL" "jq is required to inspect Prometheus alertmanager discovery JSON"
    return
  fi

  api_status="$(jq -r '.status // "unknown"' "$TMP_ALERTMANAGERS" 2>/dev/null || printf 'invalid')"
  if [ "$api_status" != "success" ]; then
    status_row "prometheus" "alertmanager discovery api status" "FAIL" "status=${api_status}"
    return
  fi
  status_row "prometheus" "alertmanager discovery api status" "PASS" "status=success"

  active_count="$(jq -r '(.data.activeAlertmanagers // []) | length' "$TMP_ALERTMANAGERS" 2>/dev/null || printf 'invalid')"
  case "$active_count" in
    ''|*[!0-9]*)
      status_row "prometheus" "active alertmanager targets" "FAIL" "active target count is invalid"
      return
      ;;
    0)
      status_row "prometheus" "active alertmanager targets" "FAIL" "active_count=0"
      return
      ;;
    *)
      status_row "prometheus" "active alertmanager targets" "PASS" "active_count=${active_count}"
      ;;
  esac

  if [ -z "$ALERTMANAGER_URL" ]; then
    status_row "prometheus" "expected alertmanager target" "INCOMPLETE" "ALERTMANAGER_URL is required to match Prometheus target wiring"
    return
  fi

  expected="$(base_url "$ALERTMANAGER_URL")"
  matched_count="$(jq -r --arg expected "$expected" '
    def normalize_target:
      gsub("/+$"; "")
      | sub("/api/v[0-9]+/alerts$"; "");
    [
      (.data.activeAlertmanagers // [])[]
      | (.url // "")
      | normalize_target
      | select(. == $expected)
    ]
    | length
  ' "$TMP_ALERTMANAGERS" 2>/dev/null || printf '0')"
  if [ "$matched_count" -gt 0 ] 2>/dev/null; then
    status_row "prometheus" "expected alertmanager target" "PASS" "expected Alertmanager URL is present in active Prometheus targets"
  else
    status_row "prometheus" "expected alertmanager target" "FAIL" "expected Alertmanager URL is not present in active Prometheus targets"
  fi
}

check_alertmanager_ready() {
  if [ -n "$ALERTMANAGER_READY_TEXT_FILE" ]; then
    if [ ! -r "$ALERTMANAGER_READY_TEXT_FILE" ]; then
      status_row "alertmanager" "readiness source" "FAIL" "ALERTMANAGER_READY_TEXT_FILE is not readable"
      return
    fi
    cp "$ALERTMANAGER_READY_TEXT_FILE" "$TMP_STATUS"
    status_row "alertmanager" "readiness source" "PASS" "loaded from local fixture"
    body="$(one_line < "$TMP_STATUS" || true)"
    case "$(printf '%s' "$body" | tr '[:upper:]' '[:lower:]')" in
      *ok*|*ready*) status_row "alertmanager" "ready endpoint" "PASS" "$body" ;;
      *) status_row "alertmanager" "ready endpoint" "FAIL" "unexpected readiness body" ;;
    esac
    return
  fi

  if [ -z "$ALERTMANAGER_URL" ]; then
    status_row "alertmanager" "readiness source" "INCOMPLETE" "ALERTMANAGER_URL or ALERTMANAGER_READY_TEXT_FILE is required for live route evidence"
    return
  fi

  url="$(base_url "$ALERTMANAGER_URL")/-/ready"
  if curl_capture "$url" > "$TMP_STATUS" 2>/dev/null; then
    status_row "alertmanager" "readiness source" "PASS" "queried Alertmanager /-/ready"
    body="$(one_line < "$TMP_STATUS" || true)"
    case "$(printf '%s' "$body" | tr '[:upper:]' '[:lower:]')" in
      *ok*|*ready*) status_row "alertmanager" "ready endpoint" "PASS" "$body" ;;
      *) status_row "alertmanager" "ready endpoint" "PASS" "HTTP 2xx; body not printed as evidence" ;;
    esac
  else
    status_row "alertmanager" "readiness source" "FAIL" "Alertmanager /-/ready query failed"
  fi
}

check_alertmanager_route_contract() {
  if [ -z "$ALERT_ROUTE_EXPECTED_RECEIVER" ]; then
    status_row "alertmanager" "expected on-call receiver" "INCOMPLETE" "ALERT_ROUTE_EXPECTED_RECEIVER is required"
    return
  fi

  if [ -z "$ALERT_ROUTE_JSON_FILE" ]; then
    status_row "alertmanager" "route contract" "INCOMPLETE" "ALERT_ROUTE_JSON_FILE is required; do not fetch or print raw Alertmanager config"
    return
  fi

  if [ ! -r "$ALERT_ROUTE_JSON_FILE" ]; then
    status_row "alertmanager" "route contract" "FAIL" "ALERT_ROUTE_JSON_FILE is not readable"
    return
  fi

  if ! command -v jq >/dev/null 2>&1; then
    status_row "alertmanager" "jq available" "FAIL" "jq is required to inspect alert route contract JSON"
    return
  fi

  route_count="$(jq -r '(.routes // []) | length' "$ALERT_ROUTE_JSON_FILE" 2>/dev/null || printf 'invalid')"
  if [ "$route_count" != "1" ]; then
    status_row "alertmanager" "route contract cardinality" "FAIL" "expected exactly one sanitized route object; found ${route_count}"
    return
  fi
  status_row "alertmanager" "route contract cardinality" "PASS" "one sanitized route object"

  if jq -e --arg receiver "$ALERT_ROUTE_EXPECTED_RECEIVER" '
    (.routes // [])[0]
    | (
        (.receiver // "") == $receiver
        and ((.matchers // []) | any(.[]?; test("^component\\s*=\\s*\"?orchestration\"?$")))
      )
  ' "$ALERT_ROUTE_JSON_FILE" >/dev/null 2>&1; then
    status_row "alertmanager" "orchestration route receiver" "PASS" "expected receiver and component=orchestration matcher found on the same sanitized route"
  else
    status_row "alertmanager" "orchestration route receiver" "FAIL" "expected receiver and component=orchestration matcher were not found on the same sanitized route"
  fi

  has_receivers="$(jq -r 'has("receivers") and ((.receivers | type) == "array")' "$ALERT_ROUTE_JSON_FILE" 2>/dev/null || printf 'false')"
  if [ "$has_receivers" != "true" ]; then
    status_row "alertmanager" "receiver integration summary" "FAIL" "sanitized contract is missing receivers[] integration summary"
    return
  fi

  receiver_count="$(jq -r --arg receiver "$ALERT_ROUTE_EXPECTED_RECEIVER" '
    [
      (.receivers // [])[]
      | select(((.name // .receiver // "") == $receiver))
    ]
    | length
  ' "$ALERT_ROUTE_JSON_FILE" 2>/dev/null || printf '0')"
  if [ "$receiver_count" = "0" ]; then
    status_row "alertmanager" "receiver integration summary" "FAIL" "expected receiver was not found in sanitized receivers[]"
    return
  fi

  integration_count="$(jq -r --arg receiver "$ALERT_ROUTE_EXPECTED_RECEIVER" '
    [
      (.receivers // [])[]
      | select(((.name // .receiver // "") == $receiver))
      | ((.integration_count // .integrationCount // 0) | tonumber? // 0)
    ]
    | max // 0
  ' "$ALERT_ROUTE_JSON_FILE" 2>/dev/null || printf '0')"
  if [ "$integration_count" -gt 0 ] 2>/dev/null; then
    status_row "alertmanager" "receiver integration summary" "PASS" "expected receiver has sanitized integration_count=${integration_count}"
  else
    status_row "alertmanager" "receiver integration summary" "FAIL" "expected receiver has no sanitized notification integrations"
  fi
}

write_header() {
  captured_at="$(date -u '+%Y-%m-%dT%H:%M:%SZ')"
  repo_sha="$(git rev-parse HEAD 2>/dev/null || printf 'unknown')"

  append "# Orchestration Alert Route Check"
  append ""
  append "- target: ${TARGET_NAME}"
  append "- captured_at_utc: ${captured_at}"
  append "- repo_sha: ${repo_sha}"
  append "- collector: scripts/release/orchestration_alert_route_check.sh"
  append "- secret_policy: bearer tokens, webhook URLs, raw Alertmanager config, notification secrets, and discovered Alertmanager target URLs are not printed"
  append ""
  append "## Checks"
  append ""
  append "| Area | Check | Status | Evidence |"
  append "| ---- | ----- | ------ | -------- |"
}

write_footer() {
  alert_route_status="PASS"
  if [ "$failures" -gt 0 ]; then
    alert_route_status="FAIL"
  elif [ "$incomplete" -gt 0 ]; then
    alert_route_status="INCOMPLETE"
  fi

  append ""
  append "## Summary"
  append ""
  append "- failures: ${failures}"
  append "- incomplete_checks: ${incomplete}"
  append "- skipped_optional_checks: ${skips}"
  append "- alert_route_status: ${alert_route_status}"
  append ""
  append "## Remaining Release-Gate Context"
  append ""
  append "This report supports alert-rule and route-contract review only. It does not prove:"
  append ""
  append "- an on-call notification was received"
  append "- the 24h staging soak stayed green"
  append "- canary or rollback evidence is complete"
  append "- dashboard trend review is complete"
  append "- Backend, SRE, Security, or Frontend owner signoff is complete"
}

main() {
  write_header
  check_prometheus_rules
  check_prometheus_alertmanager_targets
  check_alertmanager_ready
  check_alertmanager_route_contract
  write_footer

  if [ -n "$ALERT_ROUTE_OUTPUT" ]; then
    mkdir -p "$(dirname "$ALERT_ROUTE_OUTPUT")"
    cp "$TMP_MD" "$ALERT_ROUTE_OUTPUT"
    echo "Wrote orchestration alert route check: $ALERT_ROUTE_OUTPUT"
  else
    cat "$TMP_MD"
  fi

  if [ "$failures" -gt 0 ]; then
    exit 1
  fi
  if [ "$incomplete" -gt 0 ]; then
    exit 1
  fi
}

main

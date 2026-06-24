#!/usr/bin/env sh
set -eu

TARGET_NAME="${TARGET_NAME:-staging}"
SOAK_DURATION_SECONDS="${SOAK_DURATION_SECONDS:-86400}"
SOAK_INTERVAL_SECONDS="${SOAK_INTERVAL_SECONDS:-3600}"
SOAK_MIN_SAMPLES="${SOAK_MIN_SAMPLES:-2}"
SOAK_OUTPUT_DIR="${SOAK_OUTPUT_DIR:-}"
SOAK_SNAPSHOT_COMMAND="${SOAK_SNAPSHOT_COMMAND:-scripts/release/orchestration_gate_snapshot.sh}"
SOAK_REQUIRE_OPTIONAL="${SOAK_REQUIRE_OPTIONAL:-false}"
SOAK_ALLOW_SHORT="${SOAK_ALLOW_SHORT:-false}"

usage() {
  cat <<'EOF'
Usage: scripts/release/orchestration_soak_runner.sh

Run repeated orchestration release-gate snapshots and write a soak evidence
summary for staging soak and canary readiness workflows.

Environment:
  TARGET_NAME                 Label printed in reports. Default: staging
  SOAK_DURATION_SECONDS       Total wall-clock duration. Default: 86400
  SOAK_INTERVAL_SECONDS       Delay between snapshots. Default: 3600
  SOAK_MIN_SAMPLES            Minimum snapshots required for success. Default: 2
  SOAK_OUTPUT_DIR             Output directory. Default: /tmp/agentforge-orchestration-soak-<timestamp>
  SOAK_SNAPSHOT_COMMAND       Snapshot command path. Default: scripts/release/orchestration_gate_snapshot.sh
  SOAK_REQUIRE_OPTIONAL       Passed to SNAPSHOT_REQUIRE_OPTIONAL. Default: false
  SOAK_ALLOW_SHORT            true => allow <24h smoke runs to exit 0. Default: false

The snapshot command receives the same environment expected by
orchestration_gate_snapshot.sh, such as API_BASE_URL, ORCHESTRATOR_BASE_URL,
NATS_MONITOR_URL, METRICS_BEARER_TOKEN, and DATABASE_URL. Do not place secrets
directly in SOAK_SNAPSHOT_COMMAND.

The generated summary is supporting evidence only. It cannot replace dashboard
ownership, alert-routing confirmation, manual browser/scenario records, canary
timeline, rollback drill record, or owner signoff.
runner.log is local diagnostic output from the child command; review and redact
it before attaching it to any release evidence package.
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

is_uint() {
  case "$1" in
    ""|*[!0-9]*) return 1 ;;
    *) return 0 ;;
  esac
}

timestamp() {
  date -u '+%Y%m%dT%H%M%SZ'
}

iso_timestamp() {
  date -u '+%Y-%m-%dT%H:%M:%SZ'
}

sanitize() {
  printf '%s' "$1" | tr -c 'A-Za-z0-9._-' '-' | sed 's/--*/-/g; s/^-//; s/-$//'
}

extract_summary_value() {
  key="$1"
  file="$2"
  sed -n "s/^- ${key}: //p" "$file" 2>/dev/null | tail -n 1
}

validate_config() {
  if ! is_uint "$SOAK_DURATION_SECONDS"; then
    echo "ERROR: SOAK_DURATION_SECONDS must be a non-negative integer" >&2
    exit 2
  fi
  if ! is_uint "$SOAK_INTERVAL_SECONDS" || [ "$SOAK_INTERVAL_SECONDS" -eq 0 ]; then
    echo "ERROR: SOAK_INTERVAL_SECONDS must be a positive integer" >&2
    exit 2
  fi
  if ! is_uint "$SOAK_MIN_SAMPLES" || [ "$SOAK_MIN_SAMPLES" -eq 0 ]; then
    echo "ERROR: SOAK_MIN_SAMPLES must be a positive integer" >&2
    exit 2
  fi
  if [ ! -x "$SOAK_SNAPSHOT_COMMAND" ]; then
    echo "ERROR: SOAK_SNAPSHOT_COMMAND is not executable: $SOAK_SNAPSHOT_COMMAND" >&2
    exit 2
  fi
}

write_summary() {
  summary_file="$1"
  started_at="$2"
  ended_at="$3"
  elapsed="$4"
  total_samples="$5"
  passed_samples="$6"
  failed_samples="$7"
  total_reported_failures="$8"
  total_skips="$9"

  full_window="NO"
  if [ "$elapsed" -ge 86400 ]; then
    full_window="YES"
  fi

  short_window_allowed="NO"
  case "$SOAK_ALLOW_SHORT" in
    true|1|yes|on) short_window_allowed="YES" ;;
  esac

  min_samples_status="PASS"
  if [ "$total_samples" -lt "$SOAK_MIN_SAMPLES" ]; then
    min_samples_status="FAIL"
  fi

  snapshot_checks_outcome="PASS"
  if [ "$failed_samples" -gt 0 ] || [ "$total_reported_failures" -gt 0 ] || [ "$min_samples_status" = "FAIL" ]; then
    snapshot_checks_outcome="FAIL"
  fi

  soak_gate_status="PASS"
  if [ "$snapshot_checks_outcome" = "FAIL" ]; then
    soak_gate_status="FAIL"
  elif [ "$full_window" = "NO" ] && [ "$short_window_allowed" = "YES" ]; then
    soak_gate_status="SMOKE_ONLY"
  elif [ "$full_window" = "NO" ]; then
    soak_gate_status="INCOMPLETE"
  fi

  {
    printf '# Orchestration Staging Soak Summary\n\n'
    printf -- '- target: %s\n' "$TARGET_NAME"
    printf -- '- started_at_utc: %s\n' "$started_at"
    printf -- '- ended_at_utc: %s\n' "$ended_at"
    printf -- '- configured_duration_seconds: %s\n' "$SOAK_DURATION_SECONDS"
    printf -- '- actual_elapsed_seconds: %s\n' "$elapsed"
    printf -- '- interval_seconds: %s\n' "$SOAK_INTERVAL_SECONDS"
    printf -- '- snapshot_command: %s\n' "$SOAK_SNAPSHOT_COMMAND"
    printf -- '- output_dir: %s\n' "$SOAK_OUTPUT_DIR"
    printf -- '- full_24h_window: %s\n' "$full_window"
    printf -- '- short_window_allowed: %s\n' "$short_window_allowed"
    printf -- '- min_samples_required: %s\n' "$SOAK_MIN_SAMPLES"
    printf -- '- min_samples_status: %s\n' "$min_samples_status"
    printf -- '- total_samples: %s\n' "$total_samples"
    printf -- '- passed_samples: %s\n' "$passed_samples"
    printf -- '- failed_samples: %s\n' "$failed_samples"
    printf -- '- reported_snapshot_failures: %s\n' "$total_reported_failures"
    printf -- '- skipped_optional_checks: %s\n' "$total_skips"
    printf -- '- snapshot_checks_outcome: %s\n' "$snapshot_checks_outcome"
    printf -- '- soak_gate_status: %s\n\n' "$soak_gate_status"

    printf '## Samples\n\n'
    printf '| # | Captured At UTC | Status | Reported Failures | Skipped Optional Checks | Snapshot |\n'
    printf '| - | --------------- | ------ | ----------------- | ----------------------- | -------- |\n'
    if [ -f "$SAMPLES_TSV" ]; then
      while IFS='|' read -r sample_index captured_at status report_failures report_skips snapshot_file; do
        [ -n "$sample_index" ] || continue
        printf '| %s | %s | %s | %s | %s | `%s` |\n' \
          "$sample_index" "$captured_at" "$status" "$report_failures" "$report_skips" "$snapshot_file"
      done < "$SAMPLES_TSV"
    fi

    printf '\n## Remaining Manual Gate Items\n\n'
    printf 'This summary is not sufficient to close the production release gate by itself. Still attach:\n\n'
    printf -- '- the full snapshot bundle for trend review\n'
    printf -- '- dashboard links or screenshots owned by SRE\n'
    printf -- '- alert routing confirmation to the on-call target\n'
    printf -- '- manual scenario records, including websocket/polling fallback\n'
    printf -- '- production canary timeline at 5%%, 25%%, and 100%%\n'
    printf -- '- rollback drill with feature flags\n'
    printf -- '- Backend, SRE, Security, and Frontend owner signoffs\n'
  } > "$summary_file"

  [ "$soak_gate_status" = "PASS" ] || [ "$soak_gate_status" = "SMOKE_ONLY" ]
}

validate_config

if [ -z "$SOAK_OUTPUT_DIR" ]; then
  SOAK_OUTPUT_DIR="/tmp/agentforge-orchestration-soak-$(sanitize "$TARGET_NAME")-$(timestamp)"
fi
mkdir -p "$SOAK_OUTPUT_DIR"

SAMPLES_TSV="$SOAK_OUTPUT_DIR/samples.tsv"
LOG_FILE="$SOAK_OUTPUT_DIR/runner.log"
SUMMARY_FILE="$SOAK_OUTPUT_DIR/summary.md"
: > "$SAMPLES_TSV"
: > "$LOG_FILE"

start_epoch="$(date -u '+%s')"
end_epoch=$((start_epoch + SOAK_DURATION_SECONDS))
started_at="$(iso_timestamp)"

sample_index=0
passed_samples=0
failed_samples=0
total_reported_failures=0
total_skips=0

echo "Starting orchestration soak runner: output_dir=$SOAK_OUTPUT_DIR"

while :; do
  sample_index=$((sample_index + 1))
  captured_at="$(iso_timestamp)"
  sample_stamp="$(timestamp)"
  sample_file="$SOAK_OUTPUT_DIR/snapshot-${sample_index}-${sample_stamp}.md"

  echo "[$captured_at] collecting snapshot #$sample_index -> $sample_file"

  if SNAPSHOT_OUTPUT="$sample_file" SNAPSHOT_REQUIRE_OPTIONAL="$SOAK_REQUIRE_OPTIONAL" "$SOAK_SNAPSHOT_COMMAND" >> "$LOG_FILE" 2>&1; then
    command_status="PASS"
  else
    command_status="FAIL"
  fi

  report_failures="$(extract_summary_value "failures" "$sample_file")"
  report_skips="$(extract_summary_value "skipped_optional_checks" "$sample_file")"

  if [ -z "$report_failures" ] || ! is_uint "$report_failures"; then
    command_status="FAIL"
    report_failures=1
  fi
  if [ -z "$report_skips" ] || ! is_uint "$report_skips"; then
    command_status="FAIL"
    report_skips=0
    report_failures=$((report_failures + 1))
  fi

  if [ ! -s "$sample_file" ]; then
    command_status="FAIL"
    report_failures=$((report_failures + 1))
  fi

  total_reported_failures=$((total_reported_failures + report_failures))
  total_skips=$((total_skips + report_skips))

  if [ "$command_status" = "PASS" ] && [ "$report_failures" -eq 0 ]; then
    passed_samples=$((passed_samples + 1))
  else
    failed_samples=$((failed_samples + 1))
  fi

  printf '%s|%s|%s|%s|%s|%s\n' \
    "$sample_index" "$captured_at" "$command_status" "$report_failures" "$report_skips" "$sample_file" \
    >> "$SAMPLES_TSV"

  now_epoch="$(date -u '+%s')"
  if [ "$now_epoch" -ge "$end_epoch" ]; then
    break
  fi

  sleep_for="$SOAK_INTERVAL_SECONDS"
  remaining=$((end_epoch - now_epoch))
  if [ "$remaining" -lt "$sleep_for" ]; then
    sleep_for="$remaining"
  fi
  [ "$sleep_for" -gt 0 ] || break
  sleep "$sleep_for"
done

ended_at="$(iso_timestamp)"
end_actual_epoch="$(date -u '+%s')"
elapsed=$((end_actual_epoch - start_epoch))

if write_summary "$SUMMARY_FILE" "$started_at" "$ended_at" "$elapsed" "$sample_index" \
  "$passed_samples" "$failed_samples" "$total_reported_failures" "$total_skips"; then
  echo "Wrote orchestration soak summary: $SUMMARY_FILE"
  exit 0
fi

echo "Wrote orchestration soak summary with failures: $SUMMARY_FILE" >&2
exit 1

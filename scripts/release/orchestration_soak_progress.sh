#!/usr/bin/env sh
set -eu

SOAK_OUTPUT_DIR="${1:-${SOAK_OUTPUT_DIR:-}}"
SOAK_MIN_SAMPLES="${SOAK_MIN_SAMPLES:-24}"
SOAK_REQUIRED_SECONDS="${SOAK_REQUIRED_SECONDS:-86400}"
SOAK_PROGRESS_OUTPUT="${SOAK_PROGRESS_OUTPUT:-}"

usage() {
  cat <<'EOF'
Usage: scripts/release/orchestration_soak_progress.sh <soak-output-dir>

Summarize an in-progress orchestration soak directory without touching the
running soak runner. The output is progress evidence only; it does not replace
the final 24h soak summary, dashboard trend review, alert-routing evidence,
canary timeline, rollback drill, or owner signoffs.

Environment:
  SOAK_OUTPUT_DIR        Output directory when no positional argument is passed.
  SOAK_MIN_SAMPLES       Minimum samples required for final pass. Default: 24.
  SOAK_REQUIRED_SECONDS  Required window for final pass. Default: 86400.
  SOAK_PROGRESS_OUTPUT   Optional Markdown output path. Default: stdout.
EOF
}

case "${SOAK_OUTPUT_DIR:-}" in
  -h|--help)
    usage
    exit 0
    ;;
esac

case "${2:-}" in
  "")
    ;;
  -h|--help)
    usage
    exit 0
    ;;
  *)
    echo "ERROR: unexpected argument: $2" >&2
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

to_epoch() {
  date -u -d "$1" '+%s' 2>/dev/null || printf ''
}

write_report() {
  report_file="$1"
  {
    printf '# Orchestration Soak Progress\n\n'
    printf -- '- captured_at_utc: %s\n' "$(date -u '+%Y-%m-%dT%H:%M:%SZ')"
    printf -- '- output_dir: %s\n' "$SOAK_OUTPUT_DIR"
    printf -- '- samples_file: %s\n' "$samples_file"
    printf -- '- min_samples_required: %s\n' "$SOAK_MIN_SAMPLES"
    printf -- '- required_window_seconds: %s\n' "$SOAK_REQUIRED_SECONDS"
    printf -- '- first_sample_at_utc: %s\n' "$first_sample_at"
    printf -- '- last_sample_at_utc: %s\n' "$last_sample_at"
    printf -- '- elapsed_sample_window_seconds: %s\n' "$elapsed_window"
    printf -- '- total_samples: %s\n' "$total_samples"
    printf -- '- passed_samples: %s\n' "$passed_samples"
    printf -- '- failed_samples: %s\n' "$failed_samples"
    printf -- '- reported_snapshot_failures: %s\n' "$reported_failures"
    printf -- '- skipped_optional_checks: %s\n' "$skipped_checks"
    printf -- '- progress_status: %s\n\n' "$progress_status"

    printf '## Samples\n\n'
    printf '| # | Captured At UTC | Status | Reported Failures | Skipped Optional Checks | Snapshot |\n'
    printf '| - | --------------- | ------ | ----------------- | ----------------------- | -------- |\n'
    while IFS='|' read -r sample_index captured_at status report_failures report_skips snapshot_file; do
      [ -n "$sample_index" ] || continue
      printf '| %s | %s | %s | %s | %s | `%s` |\n' \
        "$sample_index" "$captured_at" "$status" "$report_failures" "$report_skips" "$snapshot_file"
    done < "$samples_file"

    printf '\n## Remaining Gate Context\n\n'
    printf 'This progress report is not sufficient to close the release gate. Still required:\n\n'
    printf -- '- full 24h soak summary and trend review\n'
    printf -- '- alert routing confirmation to the approved on-call target\n'
    printf -- '- production canary timeline at 5%%, 25%%, and 100%%\n'
    printf -- '- rollback drill summary\n'
    printf -- '- final dashboard links or screenshots\n'
    printf -- '- Backend, SRE, Security, and Frontend owner signoffs\n'
  } > "$report_file"
}

if [ -z "${SOAK_OUTPUT_DIR:-}" ]; then
  echo "ERROR: SOAK_OUTPUT_DIR or <soak-output-dir> is required" >&2
  usage >&2
  exit 2
fi

if ! is_uint "$SOAK_MIN_SAMPLES" || [ "$SOAK_MIN_SAMPLES" -eq 0 ]; then
  echo "ERROR: SOAK_MIN_SAMPLES must be a positive integer" >&2
  exit 2
fi

if ! is_uint "$SOAK_REQUIRED_SECONDS"; then
  echo "ERROR: SOAK_REQUIRED_SECONDS must be a non-negative integer" >&2
  exit 2
fi

samples_file="$SOAK_OUTPUT_DIR/samples.tsv"
if [ ! -r "$samples_file" ]; then
  echo "ERROR: samples file is not readable: $samples_file" >&2
  exit 1
fi

total_samples=0
passed_samples=0
failed_samples=0
reported_failures=0
skipped_checks=0
first_sample_at=""
last_sample_at=""

while IFS='|' read -r sample_index captured_at status report_failures report_skips _snapshot_file; do
  [ -n "$sample_index" ] || continue
  if ! is_uint "$sample_index"; then
    echo "ERROR: invalid sample index in $samples_file: $sample_index" >&2
    exit 1
  fi
  if ! is_uint "$report_failures"; then
    echo "ERROR: invalid reported failure count for sample $sample_index" >&2
    exit 1
  fi
  if ! is_uint "$report_skips"; then
    echo "ERROR: invalid skipped check count for sample $sample_index" >&2
    exit 1
  fi

  total_samples=$((total_samples + 1))
  [ -n "$first_sample_at" ] || first_sample_at="$captured_at"
  last_sample_at="$captured_at"
  reported_failures=$((reported_failures + report_failures))
  skipped_checks=$((skipped_checks + report_skips))

  if [ "$status" = "PASS" ] && [ "$report_failures" -eq 0 ]; then
    passed_samples=$((passed_samples + 1))
  else
    failed_samples=$((failed_samples + 1))
  fi
done < "$samples_file"

if [ "$total_samples" -eq 0 ]; then
  echo "ERROR: no samples found in $samples_file" >&2
  exit 1
fi

elapsed_window="unknown"
first_epoch="$(to_epoch "$first_sample_at")"
last_epoch="$(to_epoch "$last_sample_at")"
if [ -n "$first_epoch" ] && [ -n "$last_epoch" ] && [ "$last_epoch" -ge "$first_epoch" ]; then
  elapsed_window=$((last_epoch - first_epoch))
fi

progress_status="INCOMPLETE"
if [ "$failed_samples" -gt 0 ] || [ "$reported_failures" -gt 0 ]; then
  progress_status="FAIL"
elif is_uint "$elapsed_window" && [ "$elapsed_window" -ge "$SOAK_REQUIRED_SECONDS" ] && [ "$total_samples" -ge "$SOAK_MIN_SAMPLES" ]; then
  progress_status="PASS"
fi

if [ -n "$SOAK_PROGRESS_OUTPUT" ]; then
  mkdir -p "$(dirname "$SOAK_PROGRESS_OUTPUT")"
  write_report "$SOAK_PROGRESS_OUTPUT"
  echo "Wrote orchestration soak progress: $SOAK_PROGRESS_OUTPUT"
else
  tmp_report="$(mktemp)"
  write_report "$tmp_report"
  cat "$tmp_report"
  rm -f "$tmp_report"
fi

[ "$progress_status" = "FAIL" ] && exit 1
exit 0

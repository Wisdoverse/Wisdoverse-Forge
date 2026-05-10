#!/bin/bash
# =============================================================================
# Staging soak watcher
# =============================================================================
# Monitors a staging deployment for a configurable window before clearing it
# for production promotion. Runs a health-probe loop, tracks per-window error
# rates, and exits 0 only when the entire window stays clean. Designed to gate
# `gh workflow run release` (or its GitLab equivalent) by chaining: staging
# deploy → soak → production deploy.
#
# Default window: 24h. CLI publish workflows in CI shouldn't actually run for
# 24h — instead, use a scheduled job that picks up the latest main commit's
# staging deploy timestamp and confirms the soak window has elapsed before
# allowing promotion.
#
# Env (override with explicit values for shorter dev cycles):
#   SOAK_TARGET_URL        Health endpoint (default: https://coding.itoy.dev/api/health)
#   SOAK_DURATION_SECONDS  Total window length (default: 86400 = 24h)
#   SOAK_INTERVAL_SECONDS  Probe interval (default: 60)
#   SOAK_MAX_FAILURES      Cumulative failures before aborting (default: 3)
#   SOAK_HTTP_TIMEOUT      Per-probe curl timeout (default: 10)
#
# Output: streams `[soak]` log lines, exits 0 on clean window, 1 on
# `SOAK_MAX_FAILURES` exceeded. Wrap in `nohup` or systemd unit for unattended
# runs; tail the log when chaining a CI promotion gate.
# =============================================================================

set -euo pipefail

SOAK_TARGET_URL="${SOAK_TARGET_URL:-https://coding.itoy.dev/api/health}"
SOAK_DURATION_SECONDS="${SOAK_DURATION_SECONDS:-86400}"
SOAK_INTERVAL_SECONDS="${SOAK_INTERVAL_SECONDS:-60}"
SOAK_MAX_FAILURES="${SOAK_MAX_FAILURES:-3}"
SOAK_HTTP_TIMEOUT="${SOAK_HTTP_TIMEOUT:-10}"

log() { echo "$(date -u '+%Y-%m-%dT%H:%M:%SZ') [soak] $*"; }
log_error() { echo "$(date -u '+%Y-%m-%dT%H:%M:%SZ') [soak] ERROR: $*" >&2; }

start="$(date +%s)"
end="$((start + SOAK_DURATION_SECONDS))"
failures=0
probes=0

log "Soaking $SOAK_TARGET_URL for ${SOAK_DURATION_SECONDS}s (interval ${SOAK_INTERVAL_SECONDS}s, max failures ${SOAK_MAX_FAILURES})"

while true; do
  now="$(date +%s)"
  if [ "$now" -ge "$end" ]; then
    log "Soak window complete: $probes probes, $failures failures"
    exit 0
  fi

  probes=$((probes + 1))
  if curl -fsS --max-time "$SOAK_HTTP_TIMEOUT" "$SOAK_TARGET_URL" >/dev/null 2>&1; then
    if [ $((probes % 30)) -eq 0 ]; then
      log "$probes probes ok, $failures failures, $((end - now))s remaining"
    fi
  else
    failures=$((failures + 1))
    log_error "Probe $probes failed against $SOAK_TARGET_URL ($failures/$SOAK_MAX_FAILURES)"
    if [ "$failures" -ge "$SOAK_MAX_FAILURES" ]; then
      log_error "Aborting: max failures reached"
      exit 1
    fi
  fi

  sleep "$SOAK_INTERVAL_SECONDS"
done

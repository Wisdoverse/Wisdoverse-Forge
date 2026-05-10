#!/bin/bash
# =============================================================================
# Pre-deploy validator for NATS-backed orchestration env vars
# =============================================================================
# Called by `scripts/deploy.sh` when present. NATS-free deployments may delete
# this file or replace it with a no-op — `deploy.sh` only invokes it when the
# script is present and executable.
#
# When any of the orchestration rollout flags below are enabled, this script
# verifies the matching credentials/seeds are also present so the server does
# not crash in an inconsistent state at boot.
#
# Exits 0 when validation passes (or none of the orchestration flags are
# enabled). Exits 1 with a descriptive error on missing or malformed values.
# =============================================================================

set -euo pipefail

LOG_PREFIX="${LOG_PREFIX:-[deploy:nats-env]}"

log_error() { echo "$(date '+%H:%M:%S') $LOG_PREFIX ERROR: $*" >&2; }

flag_enabled() {
  local var_name="$1"
  local value="${!var_name:-}"

  value="$(printf '%s' "$value" | tr '[:upper:]' '[:lower:]')"
  case "$value" in
    "" | true | 1 | yes | on) return 0 ;;
    false | 0 | no | off) return 1 ;;
    *) return 1 ;;
  esac
}

require_var() {
  local var_name="$1"
  if [ -z "${!var_name:-}" ]; then
    log_error "$var_name is required for NATS-backed deployment"
    return 1
  fi
}

# Skip validation entirely when no NATS-backed worker is enabled.
if ! flag_enabled ORCHESTRATION_RESULT_CONSUMER_ENABLED \
  && ! flag_enabled ORCHESTRATION_ASSIGNMENT_OUTBOX_PUBLISHER_ENABLED \
  && ! flag_enabled ORCHESTRATION_PARTICIPANT_LIVENESS_ENABLED \
  && ! flag_enabled ORCHESTRATION_WS_PROJECTOR_ENABLED; then
  exit 0
fi

errors=0
for key in \
  NATS_BACKEND_PASSWORD \
  NATS_AUTH_SERVICE_PASSWORD \
  NATS_SYS_PASSWORD \
  NATS_CALLOUT_ISSUER_SEED \
  NATS_CALLOUT_ACCOUNT_SIGNING_KEY_SEED \
  NATS_CALLOUT_XKEY_SEED \
  NATS_CALLOUT_ISSUER_PUBLIC \
  NATS_CALLOUT_XKEY_PUBLIC; do
  if ! require_var "$key"; then
    errors=$((errors + 1))
  fi
done

# When the operator overrides NATS_URL, ensure they kept the backend user
# credentials — the auth callout service expects exactly that user.
if [ -n "${NATS_URL:-}" ]; then
  case "$NATS_URL" in
    *://backend:*@*) ;;
    *)
      log_error "NATS_URL overrides the compose backend default but does not use backend user credentials; remove NATS_URL or set it to nats://backend:<password>@<host>:<port>"
      errors=$((errors + 1))
      ;;
  esac
fi

if [ "$errors" -gt 0 ]; then
  exit 1
fi

exit 0

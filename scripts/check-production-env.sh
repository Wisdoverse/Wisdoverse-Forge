#!/bin/sh
set -eu

error_count=0

log_error() {
  echo "[prod-env-check] ERROR: $*" >&2
  error_count=$((error_count + 1))
}

log_info() {
  echo "[prod-env-check] INFO: $*"
}

# Uses eval for indirect variable expansion. Safe: var_name always comes from
# hardcoded strings in require_var/validate_* calls, never from user input.
get_var() {
  var_name="$1"
  case "$var_name" in
    *[!A-Za-z0-9_]*)
      log_error "Invalid variable name: $var_name"
      return 1
      ;;
  esac
  eval "printf '%s' \"\${$var_name-}\""
}

require_var() {
  var_name="$1"
  value="$(get_var "$var_name")"
  if [ -z "$value" ]; then
    log_error "$var_name is required for production deploy"
  fi
}

matches_regex() {
  value="$1"
  regex="$2"
  printf '%s' "$value" | grep -Eq "$regex"
}

validate_required() {
  require_var JWT_SECRET
  require_var API_KEY_SALT
  require_var LLM_ENCRYPTION_KEY
  require_var DATABASE_URL
  require_var REDIS_URL
  require_var APP_URL
  require_var TRUST_PROXY
  require_var ALLOWED_ORIGINS
  require_var CONTAINER_ALLOWED_MOUNT_PREFIXES
}

validate_formats() {
  jwt_secret="$(get_var JWT_SECRET)"
  if [ -n "$jwt_secret" ] && [ "${#jwt_secret}" -lt 43 ]; then
    log_error "JWT_SECRET must be at least 43 characters (256 bits for HS256)"
  fi

  api_key_salt="$(get_var API_KEY_SALT)"
  if [ -n "$api_key_salt" ] && [ "${#api_key_salt}" -lt 16 ]; then
    log_error "API_KEY_SALT must be at least 16 characters"
  fi

  llm_key="$(get_var LLM_ENCRYPTION_KEY)"
  if [ -n "$llm_key" ] && ! matches_regex "$llm_key" '^[0-9a-fA-F]{64}$'; then
    log_error "LLM_ENCRYPTION_KEY must be 64 hex characters"
  fi

  database_url="$(get_var DATABASE_URL)"
  if [ -n "$database_url" ]; then
    case "$database_url" in
      postgres://*|postgresql://*) ;;
      *) log_error "DATABASE_URL must start with postgres:// or postgresql://" ;;
    esac
  fi

  redis_url="$(get_var REDIS_URL)"
  if [ -n "$redis_url" ]; then
    case "$redis_url" in
      redis://*|rediss://*) ;;
      *) log_error "REDIS_URL must start with redis:// or rediss://" ;;
    esac
  fi

  app_url="$(get_var APP_URL)"
  if [ -n "$app_url" ]; then
    case "$app_url" in
      https://*) ;;
      *) log_error "APP_URL must use https:// in production" ;;
    esac
  fi

  trust_proxy="$(get_var TRUST_PROXY | tr '[:upper:]' '[:lower:]')"
  if [ -n "$trust_proxy" ]; then
    case "$trust_proxy" in
      true|1|yes) ;;
      *) log_error "TRUST_PROXY must be true in production" ;;
    esac
  fi

  allowed_origins="$(get_var ALLOWED_ORIGINS)"
  if [ -n "$allowed_origins" ]; then
    if matches_regex "$allowed_origins" '(^|,)[[:space:]]*\*([[:space:]]*,|$)'; then
      log_error "ALLOWED_ORIGINS must not contain wildcard (*) in production"
    fi
  fi
}

validate_mount_prefixes() {
  prefixes_raw="$(get_var CONTAINER_ALLOWED_MOUNT_PREFIXES)"
  workspace_root="${AGENTFORGE_WORKSPACE_ROOT:-/data/agentforge/workspaces}"

  old_ifs="$IFS"
  IFS=','
  # shellcheck disable=SC2086
  set -- $prefixes_raw
  IFS="$old_ifs"

  if [ "$#" -eq 0 ]; then
    log_error "CONTAINER_ALLOWED_MOUNT_PREFIXES must contain at least one path"
    return
  fi

  found_workspace_root=0
  for raw in "$@"; do
    prefix="$(printf '%s' "$raw" | sed 's/^[[:space:]]*//;s/[[:space:]]*$//')"

    if [ -z "$prefix" ]; then
      log_error "CONTAINER_ALLOWED_MOUNT_PREFIXES contains an empty entry"
      continue
    fi
    if [ "$prefix" = "/" ]; then
      log_error "CONTAINER_ALLOWED_MOUNT_PREFIXES must not include '/'"
      continue
    fi
    case "$prefix" in
      /*) ;;
      *)
        log_error "CONTAINER_ALLOWED_MOUNT_PREFIXES entry must be absolute: $prefix"
        continue
        ;;
    esac

    if [ "$prefix" = "$workspace_root" ]; then
      found_workspace_root=1
    fi
  done

  if [ "$found_workspace_root" -ne 1 ]; then
    log_error "CONTAINER_ALLOWED_MOUNT_PREFIXES must include AGENTFORGE_WORKSPACE_ROOT ($workspace_root)"
  fi
}

validate_bool_flag() {
  var_name="$1"
  value="$(get_var "$var_name" | tr '[:upper:]' '[:lower:]')"
  if [ -z "$value" ]; then
    return
  fi
  case "$value" in
    true|false|1|0|yes|no|on|off) ;;
    *) log_error "$var_name must be boolean when set (true/false/1/0/yes/no/on/off)" ;;
  esac
}

flag_enabled() {
  var_name="$1"
  value="$(get_var "$var_name" | tr '[:upper:]' '[:lower:]')"
  case "$value" in
    ''|true|1|yes|on) return 0 ;;
    false|0|no|off) return 1 ;;
    *) return 1 ;;
  esac
}

validate_nats_url_contract() {
  nats_url="$(get_var NATS_URL)"
  if [ -z "$nats_url" ]; then
    return
  fi

  case "$nats_url" in
    *://backend:*@*) ;;
    *)
      log_error "NATS_URL must use backend user credentials, or be unset so compose derives it from NATS_BACKEND_PASSWORD"
      ;;
  esac
}

validate_nats_password_config_safe() {
  var_name="$1"
  value="$(get_var "$var_name")"
  if [ -z "$value" ]; then
    return
  fi
  if ! matches_regex "$value" '^[A-Za-z_][A-Za-z0-9_.@%+=:-]*$'; then
    log_error "$var_name must start with a letter or underscore and use only URL-safe characters because docker/nats.conf expands it unquoted"
  fi
}

validate_orchestration_flags() {
  validate_bool_flag ORCHESTRATION_RESULT_CONSUMER_ENABLED
  validate_bool_flag ORCHESTRATION_ASSIGNMENT_OUTBOX_PUBLISHER_ENABLED
  validate_bool_flag ORCHESTRATION_PARTICIPANT_LIVENESS_ENABLED
  validate_bool_flag ORCHESTRATION_CONTROL_PLANE_METRICS_ENABLED
  validate_bool_flag ORCHESTRATION_WS_PROJECTOR_ENABLED

  if flag_enabled ORCHESTRATION_RESULT_CONSUMER_ENABLED \
    || flag_enabled ORCHESTRATION_ASSIGNMENT_OUTBOX_PUBLISHER_ENABLED \
    || flag_enabled ORCHESTRATION_PARTICIPANT_LIVENESS_ENABLED \
    || flag_enabled ORCHESTRATION_WS_PROJECTOR_ENABLED; then
    require_var NATS_BACKEND_PASSWORD
    require_var NATS_AUTH_SERVICE_PASSWORD
    require_var NATS_SYS_PASSWORD
    require_var NATS_CALLOUT_ISSUER_SEED
    require_var NATS_CALLOUT_ACCOUNT_SIGNING_KEY_SEED
    require_var NATS_CALLOUT_XKEY_SEED
    require_var NATS_CALLOUT_ISSUER_PUBLIC
    require_var NATS_CALLOUT_XKEY_PUBLIC
    validate_nats_password_config_safe NATS_BACKEND_PASSWORD
    validate_nats_password_config_safe NATS_AUTH_SERVICE_PASSWORD
    validate_nats_password_config_safe NATS_SYS_PASSWORD
    validate_nats_url_contract
  fi
}

main() {
  log_info "Validating production deployment environment variables"

  validate_required
  validate_formats
  validate_mount_prefixes
  validate_orchestration_flags

  if [ "$error_count" -gt 0 ]; then
    log_error "Validation failed with $error_count error(s)"
    exit 1
  fi

  log_info "Production environment validation passed"
}

main "$@"

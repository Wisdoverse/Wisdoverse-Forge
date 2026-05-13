#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
AUDIT_TMP_DIR="$(mktemp -d)"
KEEP_AUDIT_TMP=0

DOMAIN="${DOMAIN:-localhost}"
BASE_URL="${BASE_URL:-}"
AGENT_REGISTRY="${AGENT_REGISTRY:-ghcr.io/wisdoverse/wisdoverse-forge}"
GHCR_IMAGE_TAG="${GHCR_IMAGE_TAG:-main}"
PULL_IMAGES=0
CHECK_LIVE=0
CHECK_PROVIDER=0
LOCAL_SMOKE=0
LOCAL_SMOKE_CLEANUP=0
LOCAL_SMOKE_PROJECT=""
LOCAL_SMOKE_ENV_FILE=""
LOCAL_SMOKE_OAUTH_DIR=""
LOCAL_SMOKE_AGENT_NETWORK=""
LOCAL_SMOKE_HTTP_PORT=""
LOCAL_SMOKE_HTTPS_PORT=""
LOCAL_SMOKE_AGENTFORGE_HOST_PORT=""
LOCAL_SMOKE_DB_PORT=""
LOCAL_SMOKE_REDIS_PORT=""
LOCAL_SMOKE_NATS_PORT=""
LOCAL_SMOKE_NATS_MONITOR_PORT=""
LOCAL_SMOKE_TEMPORAL_PORT=""
LOCAL_SMOKE_TEMPORAL_UI_PORT=""
LOCAL_SMOKE_ORCHESTRATOR_PORT=""

cleanup() {
  if [ "$LOCAL_SMOKE_CLEANUP" -eq 1 ]; then
    smoke_compose down -v --remove-orphans >/dev/null 2>&1 || true
    docker network rm "$LOCAL_SMOKE_AGENT_NETWORK" >/dev/null 2>&1 || true
  fi

  if [ "$KEEP_AUDIT_TMP" -eq 0 ]; then
    rm -rf "$AUDIT_TMP_DIR"
  fi
}

trap cleanup EXIT

usage() {
  cat <<'USAGE'
Audit the beginner self-host path.

Default checks are non-destructive and do not require a running stack:
  - beginner Make targets exist
  - self-host bootstrap can create a fresh env file
  - required production secrets are generated
  - prod Compose and Caddy config validate
  - bootstrap does not depend on the local Node/npm development path

Optional checks:
  --pull-images  Pull GHCR server/frontend/agent images.
  --live         Check the live public URL with scripts/check-selfhost-runtime.sh.
  --local-smoke  Start an isolated localhost self-host stack, verify it, then stop it.
  --provider     Exercise a real provider key and Provider+Prompt agent.

Provider audit env:
  BASE_URL                  Public app URL, e.g. https://forge.example.com
  E2E_EMAIL                 Login email
  E2E_PASSWORD              Login password
  BEGINNER_PROVIDER         Provider key, e.g. openai or openrouter
  BEGINNER_MODEL            Model name accepted by that provider
  BEGINNER_API_KEY          Real provider API key
  BEGINNER_BASE_URL         Optional provider base URL

Examples:
  scripts/audit-beginner-selfhost.sh
  scripts/audit-beginner-selfhost.sh --pull-images
  scripts/audit-beginner-selfhost.sh --local-smoke
  BASE_URL=https://forge.example.com E2E_EMAIL=dev@example.com E2E_PASSWORD=... \
    BEGINNER_PROVIDER=openrouter BEGINNER_MODEL=openai/gpt-4o-mini \
    BEGINNER_API_KEY=... scripts/audit-beginner-selfhost.sh --live --provider
USAGE
}

log() {
  printf '[beginner-audit] %s\n' "$*"
}

pass() {
  printf '[beginner-audit] PASS: %s\n' "$*"
}

warn() {
  printf '[beginner-audit] SKIP: %s\n' "$*"
}

fail() {
  printf '[beginner-audit] FAIL: %s\n' "$*" >&2
  printf '[beginner-audit] diagnostics kept at %s\n' "$AUDIT_TMP_DIR" >&2
  if [ "$LOCAL_SMOKE_CLEANUP" -eq 1 ]; then
    printf '[beginner-audit] local smoke containers kept with prefix %s\n' "$LOCAL_SMOKE_PROJECT" >&2
    LOCAL_SMOKE_CLEANUP=0
  fi
  KEEP_AUDIT_TMP=1
  exit 1
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --pull-images)
      PULL_IMAGES=1
      ;;
    --live)
      CHECK_LIVE=1
      ;;
    --local-smoke)
      LOCAL_SMOKE=1
      ;;
    --provider)
      CHECK_PROVIDER=1
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      usage >&2
      fail "unknown argument: $1"
      ;;
  esac
  shift
done

require_cmd() {
  command -v "$1" >/dev/null 2>&1 || fail "missing required command: $1"
}

require_env() {
  local key="$1"
  if [ -z "${!key:-}" ]; then
    fail "$key is required"
  fi
}

json_string() {
  jq -Rn --arg value "$1" '$value'
}

api_base() {
  printf '%s/api/v1' "${BASE_URL%/}"
}

curl_json() {
  local method="$1"
  local path="$2"
  local token="${3:-}"
  local body="${4:-}"
  local out="$5"
  local status
  local args=(-sS -o "$out" -w '%{http_code}' -X "$method" "$(api_base)$path")

  if [ -n "$token" ]; then
    args+=(-H "authorization: Bearer $token")
  fi

  if [ -n "$body" ]; then
    args+=(-H 'content-type: application/json' --data "$body")
  fi
  status="$(
    curl "${args[@]}"
  )"

  printf '%s' "$status"
}

smoke_env() {
  env \
    COMPOSE_ENV_FILE="$LOCAL_SMOKE_ENV_FILE" \
    COMPOSE_PROJECT_NAME="$LOCAL_SMOKE_PROJECT" \
    CONTAINER_NAME_PREFIX="$LOCAL_SMOKE_PROJECT" \
    CONTAINER_NETWORK="$LOCAL_SMOKE_AGENT_NETWORK" \
    OAUTH_MOUNT_DIR="$LOCAL_SMOKE_OAUTH_DIR" \
    HTTP_PORT="$LOCAL_SMOKE_HTTP_PORT" \
    HTTPS_PORT="$LOCAL_SMOKE_HTTPS_PORT" \
    AGENTFORGE_HOST_PORT="$LOCAL_SMOKE_AGENTFORGE_HOST_PORT" \
    DB_EXPOSED_PORT="$LOCAL_SMOKE_DB_PORT" \
    REDIS_EXPOSED_PORT="$LOCAL_SMOKE_REDIS_PORT" \
    NATS_PORT="$LOCAL_SMOKE_NATS_PORT" \
    NATS_MONITOR_PORT="$LOCAL_SMOKE_NATS_MONITOR_PORT" \
    TEMPORAL_PORT="$LOCAL_SMOKE_TEMPORAL_PORT" \
    TEMPORAL_UI_PORT="$LOCAL_SMOKE_TEMPORAL_UI_PORT" \
    ORCHESTRATOR_PORT="$LOCAL_SMOKE_ORCHESTRATOR_PORT" \
    AGENT_REGISTRY="$AGENT_REGISTRY" \
    GHCR_IMAGE_TAG="$GHCR_IMAGE_TAG" \
    "$@"
}

smoke_make() {
  (cd "$ROOT_DIR" && smoke_env make "$@")
}

smoke_compose() {
  (cd "$ROOT_DIR" && smoke_env docker compose --env-file "$LOCAL_SMOKE_ENV_FILE" \
    -f docker/compose.yml -f docker/compose.prod.yml --profile prod "$@")
}

check_targets() {
  grep -q '^quickstart-selfhost-pull:' "$ROOT_DIR/Makefile" || fail "missing quickstart-selfhost-pull target"
  grep -q '^prod-pull:' "$ROOT_DIR/Makefile" || fail "missing prod-pull target"
  pass "beginner self-host Make targets exist"
}

check_bootstrap_independence() {
  if grep -Eq 'bootstrap-local|npm|node' "$ROOT_DIR/scripts/bootstrap-selfhost.sh"; then
    fail "bootstrap-selfhost.sh depends on local development bootstrap or Node/npm"
  fi
  pass "self-host bootstrap is independent of Node/npm local development setup"
}

check_temp_bootstrap() {
  local tmp_env bootstrap_out check_out
  tmp_env="$AUDIT_TMP_DIR/selfhost.env"
  bootstrap_out="$AUDIT_TMP_DIR/selfhost-bootstrap.out"
  check_out="$AUDIT_TMP_DIR/selfhost-check.out"

  ENV_FILE="$tmp_env" "$ROOT_DIR/scripts/bootstrap-selfhost.sh" --domain "$DOMAIN" >"$bootstrap_out"
  ENV_FILE="$tmp_env" "$ROOT_DIR/scripts/bootstrap-selfhost.sh" --check --domain "$DOMAIN" >"$check_out"

  for key in \
    APP_HOST \
    APP_URL \
    CORS_ORIGIN \
    POSTGRES_PASSWORD \
    REDIS_PASSWORD \
    JWT_SECRET \
    MCP_TOKEN \
    API_KEY_SALT \
    LLM_ENCRYPTION_KEY \
    NATS_BACKEND_PASSWORD \
    NATS_AUTH_SERVICE_PASSWORD \
    NATS_SYS_PASSWORD \
    NATS_CALLOUT_ISSUER_SEED \
    NATS_CALLOUT_ACCOUNT_SIGNING_KEY_SEED \
    NATS_CALLOUT_XKEY_SEED \
    NATS_CALLOUT_ISSUER_PUBLIC \
    NATS_CALLOUT_XKEY_PUBLIC
  do
    if [ -z "$(sed -n "s/^${key}=//p" "$tmp_env" | tail -n 1 | tr -d '\r')" ]; then
      fail "bootstrap did not populate $key"
    fi
  done

  grep -q 'prod Compose configuration is valid' "$check_out" ||
    fail "selfhost check did not validate prod Compose config"
  grep -q 'Caddy configuration is valid' "$check_out" ||
    fail "selfhost check did not validate Caddy"

  pass "fresh self-host env bootstraps and validates Compose/Caddy"
}

pull_images() {
  log "pulling GHCR images from $AGENT_REGISTRY tag=$GHCR_IMAGE_TAG"
  (cd "$ROOT_DIR" && make pull-server-images update-agents \
    AGENT_REGISTRY="$AGENT_REGISTRY" GHCR_IMAGE_TAG="$GHCR_IMAGE_TAG")
  pass "prebuilt server/frontend/agent images are pullable"
}

check_live() {
  local live_out
  live_out="$AUDIT_TMP_DIR/live-health.out"

  require_cmd curl
  "$ROOT_DIR/scripts/check-selfhost-runtime.sh" --wait --domain "$DOMAIN" >"$live_out" ||
    fail "live self-host health failed; see $live_out"
  pass "live public ingress health is reachable"
}

local_smoke() {
  local suffix bootstrap_out up_out health_out

  require_cmd docker
  suffix="$(date +%s)-$$"
  LOCAL_SMOKE_PROJECT="beginner-audit-$suffix"
  LOCAL_SMOKE_ENV_FILE="$AUDIT_TMP_DIR/local-smoke.env"
  LOCAL_SMOKE_OAUTH_DIR="$AUDIT_TMP_DIR/oauth-mounts"
  LOCAL_SMOKE_AGENT_NETWORK="$LOCAL_SMOKE_PROJECT-agents"
  LOCAL_SMOKE_HTTP_PORT="${BEGINNER_SMOKE_HTTP_PORT:-18080}"
  LOCAL_SMOKE_HTTPS_PORT="${BEGINNER_SMOKE_HTTPS_PORT:-18443}"
  LOCAL_SMOKE_AGENTFORGE_HOST_PORT="${BEGINNER_SMOKE_AGENTFORGE_PORT:-14003}"
  LOCAL_SMOKE_DB_PORT="${BEGINNER_SMOKE_DB_PORT:-15433}"
  LOCAL_SMOKE_REDIS_PORT="${BEGINNER_SMOKE_REDIS_PORT:-16380}"
  LOCAL_SMOKE_NATS_PORT="${BEGINNER_SMOKE_NATS_PORT:-14222}"
  LOCAL_SMOKE_NATS_MONITOR_PORT="${BEGINNER_SMOKE_NATS_MONITOR_PORT:-18223}"
  LOCAL_SMOKE_TEMPORAL_PORT="${BEGINNER_SMOKE_TEMPORAL_PORT:-17234}"
  LOCAL_SMOKE_TEMPORAL_UI_PORT="${BEGINNER_SMOKE_TEMPORAL_UI_PORT:-18234}"
  LOCAL_SMOKE_ORCHESTRATOR_PORT="${BEGINNER_SMOKE_ORCHESTRATOR_PORT:-14010}"

  bootstrap_out="$AUDIT_TMP_DIR/local-smoke-bootstrap.out"
  up_out="$AUDIT_TMP_DIR/local-smoke-up.out"
  health_out="$AUDIT_TMP_DIR/local-smoke-health.out"

  log "starting isolated localhost self-host smoke stack ($LOCAL_SMOKE_PROJECT)"
  if ! smoke_env ENV_FILE="$LOCAL_SMOKE_ENV_FILE" "$ROOT_DIR/scripts/bootstrap-selfhost.sh" \
    --domain localhost >"$bootstrap_out"; then
    fail "local smoke bootstrap failed; see $bootstrap_out"
  fi

  LOCAL_SMOKE_CLEANUP=1
  docker network create "$LOCAL_SMOKE_AGENT_NETWORK" >/dev/null
  if ! smoke_make setup pull-server-images update-agents >"$up_out"; then
    fail "local smoke image pull failed; see $up_out"
  fi
  if ! smoke_compose up -d --remove-orphans >>"$up_out"; then
    fail "local smoke compose up failed; see $up_out"
  fi

  if ! smoke_env ENV_FILE="$LOCAL_SMOKE_ENV_FILE" \
    BASE_URL="https://localhost:$LOCAL_SMOKE_HTTPS_PORT" \
    "$ROOT_DIR/scripts/check-selfhost-runtime.sh" --wait --domain localhost --insecure >"$health_out"; then
    fail "local smoke runtime health failed; see $health_out"
  fi

  smoke_compose down -v --remove-orphans >>"$up_out" || true
  docker network rm "$LOCAL_SMOKE_AGENT_NETWORK" >>"$up_out" || true
  LOCAL_SMOKE_CLEANUP=0

  pass "isolated localhost self-host stack starts, passes public ingress health, and cleans up"
}

provider_audit() {
  local provider="${BEGINNER_PROVIDER:-}"
  local model="${BEGINNER_MODEL:-}"
  local api_key="${BEGINNER_API_KEY:-}"
  local base_url="${BEGINNER_BASE_URL:-}"
  local tmp_dir register_body login_body token create_body provider_id created_provider=0 test_body list_body agent_body agent_id prompt_body
  local status test_ok stored_status content_chars

  require_cmd curl
  require_cmd jq
  require_env BASE_URL
  require_env E2E_EMAIL
  require_env E2E_PASSWORD
  [ -n "$provider" ] || fail "BEGINNER_PROVIDER is required for --provider"
  [ -n "$model" ] || fail "BEGINNER_MODEL is required for --provider"
  [ -n "$api_key" ] || fail "BEGINNER_API_KEY is required for --provider"

  tmp_dir="$AUDIT_TMP_DIR/provider"
  mkdir -p "$tmp_dir"

  register_body="$tmp_dir/register.json"
  status="$(curl_json POST '/auth/register' '' \
    "{\"email\":$(json_string "$E2E_EMAIL"),\"password\":$(json_string "$E2E_PASSWORD"),\"username\":\"beginner-audit\"}" \
    "$register_body")"
  case "$status" in
    201|409) ;;
    *) fail "register preflight failed with HTTP $status" ;;
  esac

  login_body="$tmp_dir/login.json"
  status="$(curl_json POST '/auth/login' '' \
    "{\"email\":$(json_string "$E2E_EMAIL"),\"password\":$(json_string "$E2E_PASSWORD"),\"rememberMe\":false}" \
    "$login_body")"
  [ "$status" = "200" ] || fail "login failed with HTTP $status"
  token="$(jq -r '.tokens.accessToken // .access_token // empty' "$login_body")"
  [ -n "$token" ] || fail "login response did not include an access token"

  create_body="$tmp_dir/provider-create.json"
  local provider_payload
  provider_payload="$(
    jq -n \
      --arg provider "$provider" \
      --arg model "$model" \
      --arg apiKey "$api_key" \
      --arg displayName "Beginner Audit Provider" \
      --arg baseUrl "$base_url" \
      '{provider:$provider, displayName:$displayName, model:$model, apiKey:$apiKey}
       + (if $baseUrl == "" then {} else {baseUrl:$baseUrl} end)'
  )"
  status="$(curl_json POST '/llm-providers' "$token" "$provider_payload" "$create_body")"
  if [ "$status" = "200" ]; then
    provider_id="$(jq -r '.provider.id // empty' "$create_body")"
    created_provider=1
  elif [ "$status" = "409" ]; then
    list_body="$tmp_dir/provider-list-existing.json"
    status="$(curl_json GET '/llm-providers' "$token" '' "$list_body")"
    [ "$status" = "200" ] || fail "provider list failed with HTTP $status after create conflict"
    provider_id="$(
      jq -r --arg provider "$provider" --arg model "$model" \
        '.providers[] | select(.provider == $provider and .model == $model) | .id' \
        "$list_body" | head -n 1
    )"
    [ -n "$provider_id" ] || fail "provider/model already exists but could not be found"
  else
    fail "provider create failed with HTTP $status"
  fi
  [ -n "$provider_id" ] || fail "missing provider id"

  test_body="$tmp_dir/provider-test.json"
  status="$(curl_json POST "/llm-providers/$provider_id/test" "$token" '' "$test_body")"
  [ "$status" = "200" ] || fail "provider test failed with HTTP $status"
  test_ok="$(jq -r '.ok' "$test_body")"
  if [ "$test_ok" != "true" ]; then
    fail "provider test did not pass: $(jq -c '.error // .' "$test_body")"
  fi

  list_body="$tmp_dir/provider-list.json"
  status="$(curl_json GET '/llm-providers' "$token" '' "$list_body")"
  [ "$status" = "200" ] || fail "provider list failed with HTTP $status"
  stored_status="$(
    jq -r --arg id "$provider_id" '.providers[] | select(.id == $id) | .lastTestStatus // empty' "$list_body"
  )"
  [ "$stored_status" = "passed" ] || fail "provider test status was not persisted as passed"

  agent_body="$tmp_dir/agent-create.json"
  status="$(curl_json POST '/agents' "$token" \
    "{\"name\":\"Beginner Audit Agent\",\"provider\":$(json_string "$provider"),\"model\":$(json_string "$model"),\"systemPrompt\":\"Keep responses concise.\"}" \
    "$agent_body")"
  [ "$status" = "200" ] || fail "provider agent create failed with HTTP $status"
  agent_id="$(jq -r '.agent.id // .data.id // empty' "$agent_body")"
  [ -n "$agent_id" ] || fail "missing provider agent id"

  prompt_body="$tmp_dir/prompt.sse"
  status="$(curl -sS -o "$prompt_body" -w '%{http_code}' --max-time 120 -X POST "$(api_base)/agents/$agent_id/prompt" \
    -H 'content-type: application/json' \
    -H "authorization: Bearer $token" \
    --data '{"content":"Reply with one short sentence confirming Wisdoverse Forge is ready."}')"
  [ "$status" = "200" ] || fail "provider prompt failed with HTTP $status"
  grep -q 'message_stop' "$prompt_body" || fail "provider prompt stream did not complete"
  content_chars="$(grep -ao '"content"' "$prompt_body" | wc -l | tr -d ' ')"
  [ "${content_chars:-0}" -gt 0 ] || fail "provider prompt stream did not contain assistant content"

  curl_json DELETE "/agents/$agent_id" "$token" '' "$tmp_dir/agent-delete.json" >/dev/null || true
  if [ "$created_provider" -eq 1 ]; then
    curl_json DELETE "/llm-providers/$provider_id" "$token" '' "$tmp_dir/provider-delete.json" >/dev/null || true
  fi

  pass "real provider key can test, persist status, create Provider+Prompt agent, and stream a reply"
}

main() {
  require_cmd grep
  require_cmd sed
  require_cmd docker

  check_targets
  check_bootstrap_independence
  check_temp_bootstrap

  if [ "$PULL_IMAGES" -eq 1 ]; then
    pull_images
  else
    warn "prebuilt image pull not checked; pass --pull-images to verify GHCR availability"
  fi

  if [ "$CHECK_LIVE" -eq 1 ]; then
    check_live
  else
    warn "live public ingress not checked; pass --live on the deployed VPS"
  fi

  if [ "$LOCAL_SMOKE" -eq 1 ]; then
    local_smoke
  else
    warn "isolated local self-host startup not checked; pass --local-smoke"
  fi

  if [ "$CHECK_PROVIDER" -eq 1 ]; then
    provider_audit
  else
    warn "real Provider+Prompt AI path not checked; pass --provider with a real API key"
  fi

  log "audit finished"
}

main "$@"

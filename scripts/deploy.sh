#!/bin/bash
# =============================================================================
# Wisdoverse Forge Unified Deploy Script
# =============================================================================
# Usage: deploy.sh <staging|production> [image_tag] [registry_image]
#
# Called by a release workflow or operator shell on the target server.
# Expects to run from /opt/agentforge/ with docker/ and scripts/ subdirectories.
#
# Prerequisites:
#   - PostgreSQL is externally managed, not part of compose services
#   - Server has pre-configured `docker login` to container registry
#   - Nginx must follow symlinks (no `disable_symlinks`)
#
# Features:
#   - Atomic frontend deploy via symlink swap (zero 404 window)
#   - Database migration before service start
#   - Designed to run under an external deployment mutex
#   - Health check with clear failure reporting (manual rollback in Phase 1)
#   - Agent image pull with retry + digest verification
# =============================================================================

set -euo pipefail

ENV="${1:?Usage: deploy.sh <staging|production> [image_tag] [registry_image]}"
IMAGE_TAG="${2:-}"
REGISTRY_IMAGE="${3:-}"
DEPLOY_DIR="$(cd "$(dirname "$0")/.." && pwd)"
LOG_PREFIX="[deploy:$ENV]"

# [I1] Timestamped logging + total duration tracking
log() { echo "$(date '+%H:%M:%S') $LOG_PREFIX $*"; }
log_error() { echo "$(date '+%H:%M:%S') $LOG_PREFIX ERROR: $*" >&2; }

load_compose_env() {
  local env_file="$1"
  local line key value

  [ -f "$env_file" ] || return 0

  while IFS= read -r line || [ -n "$line" ]; do
    line="${line%$'\r'}"
    case "$line" in
      "" | "#"*) continue ;;
      export\ *) line="${line#export }" ;;
    esac

    if [[ "$line" != *=* ]]; then
      log_error "Invalid docker/.env line without '=': $line"
      return 1
    fi

    key="${line%%=*}"
    value="${line#*=}"
    key="${key#"${key%%[![:space:]]*}"}"
    key="${key%"${key##*[![:space:]]}"}"

    if [[ -z "$key" || "$key" =~ [^A-Za-z0-9_] ]]; then
      log_error "Invalid docker/.env key: $key"
      return 1
    fi

    case "$value" in
      \"*\")
        value="${value#\"}"
        value="${value%\"}"
        ;;
      \'*\')
        value="${value#\'}"
        value="${value%\'}"
        ;;
    esac

    export "$key=$value"
  done <"$env_file"
}

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

require_deploy_env() {
  local var_name="$1"
  if [ -z "${!var_name:-}" ]; then
    log_error "$var_name is required for NATS-backed deployment"
    return 1
  fi
}

validate_deploy_nats_env() {
  local errors=0

  if ! flag_enabled ORCHESTRATION_RESULT_CONSUMER_ENABLED \
    && ! flag_enabled ORCHESTRATION_ASSIGNMENT_OUTBOX_PUBLISHER_ENABLED \
    && ! flag_enabled ORCHESTRATION_PARTICIPANT_LIVENESS_ENABLED \
    && ! flag_enabled ORCHESTRATION_WS_PROJECTOR_ENABLED; then
    return 0
  fi

  for key in \
    NATS_BACKEND_PASSWORD \
    NATS_AUTH_SERVICE_PASSWORD \
    NATS_SYS_PASSWORD \
    NATS_CALLOUT_ISSUER_SEED \
    NATS_CALLOUT_ACCOUNT_SIGNING_KEY_SEED \
    NATS_CALLOUT_XKEY_SEED \
    NATS_CALLOUT_ISSUER_PUBLIC \
    NATS_CALLOUT_XKEY_PUBLIC; do
    if ! require_deploy_env "$key"; then
      errors=$((errors + 1))
    fi
  done

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
}

# ---------------------------------------------------------------------------
# 0. Load environment config (docker/.env) — all environments
# ---------------------------------------------------------------------------
# docker/.env contains AGENT_REGISTRY, port overrides, and other deployment
# config. Load it as Docker Compose dotenv, not as shell code, because values
# such as `SMTP_FROM=IT <it@example.com>` are valid dotenv but invalid shell.
cd "$DEPLOY_DIR"
load_compose_env docker/.env
validate_deploy_nats_env

case "$ENV" in
  staging)
    COMPOSE_FILES="-f compose.yml -f compose.external.yml"
    # Staging keeps a safe default for older servers that predate the WEBROOT
    # variable. Production still requires an explicit value below.
    WEBROOT="${WEBROOT:-/opt/agentforge/www}"
    ;;
  production)
    COMPOSE_FILES="-f compose.yml -f compose.external.yml"
    WEBROOT="${WEBROOT:?WEBROOT environment variable must be set}"
    log "Running production environment validation..."
    ./scripts/check-production-env.sh
    ;;
  *)
    log_error "Unknown environment: $ENV"
    echo "Usage: deploy.sh <staging|production>"
    exit 1
    ;;
esac

cd "$DEPLOY_DIR/docker"

# ---------------------------------------------------------------------------
# 1. Record current image digest (audit log, not auto-rollback)
# ---------------------------------------------------------------------------
# [B2] Use `docker compose ps -q` to find container by service name,
# avoiding hardcoded container name which may have project prefix.
log "Recording current image digest for audit..."
CURRENT_CONTAINER=$(docker compose $COMPOSE_FILES ps -q agentforge-rust 2>/dev/null || echo "")
if [ -n "$CURRENT_CONTAINER" ]; then
  # shellcheck disable=SC2086
  CURRENT_IMAGE=$(docker inspect --format='{{.Config.Image}}' "$CURRENT_CONTAINER" 2>/dev/null || echo "unknown")
  log "Current image: $CURRENT_IMAGE"
else
  log "No existing deployment found (first deploy)"
fi

# ---------------------------------------------------------------------------
# 2. Ensure networks exist
# ---------------------------------------------------------------------------
log "Ensuring Docker networks..."
docker network create agentforge-agents 2>/dev/null || true
docker network create external-network 2>/dev/null || true

# ---------------------------------------------------------------------------
# 3. Pull images from registry (Build Once, Deploy Everywhere)
# ---------------------------------------------------------------------------
if [ -n "$IMAGE_TAG" ] && [ -n "$REGISTRY_IMAGE" ]; then
  log "Pulling images from registry (tag: $IMAGE_TAG)..."

  # Pull and tag Rust app image
  REMOTE_RUST_SERVER="$REGISTRY_IMAGE/rust-server:$IMAGE_TAG"
  log "Pulling $REMOTE_RUST_SERVER..."
  if ! docker pull "$REMOTE_RUST_SERVER"; then
    log_error "Failed to pull $REMOTE_RUST_SERVER"
    exit 1
  fi
  docker tag "$REMOTE_RUST_SERVER" "agentforge-rust-server:$IMAGE_TAG"
  docker tag "$REMOTE_RUST_SERVER" "agentforge-rust-server:latest"

  # Pull and tag Rust orchestrator image
  REMOTE_RUST_ORCHESTRATOR="$REGISTRY_IMAGE/rust-orchestrator:$IMAGE_TAG"
  log "Pulling $REMOTE_RUST_ORCHESTRATOR..."
  if ! docker pull "$REMOTE_RUST_ORCHESTRATOR"; then
    log_error "Failed to pull $REMOTE_RUST_ORCHESTRATOR"
    exit 1
  fi
  docker tag "$REMOTE_RUST_ORCHESTRATOR" "agentforge-orchestrator:$IMAGE_TAG"
  docker tag "$REMOTE_RUST_ORCHESTRATOR" "agentforge-orchestrator:latest"

  # Pull and tag frontend artifact image
  REMOTE_FRONTEND="$REGISTRY_IMAGE:$IMAGE_TAG"
  log "Pulling $REMOTE_FRONTEND..."
  if ! docker pull "$REMOTE_FRONTEND"; then
    log_error "Failed to pull $REMOTE_FRONTEND"
    exit 1
  fi
  docker tag "$REMOTE_FRONTEND" "agentforge-frontend:$IMAGE_TAG"
  docker tag "$REMOTE_FRONTEND" "agentforge-frontend:latest"

  log "Registry images pulled and tagged successfully"
else
  log "No registry image specified — using locally available images"
fi

# ---------------------------------------------------------------------------
# 3a. Pull agent images from registry
# ---------------------------------------------------------------------------
# Agent images are pulled separately from app/platform images.
# Claude is REQUIRED — deployment fails if it cannot be pulled or found locally.
# Other tools are best-effort: existing local images are used as fallback.
#
# Retry policy: 3 attempts with 5s backoff (handles transient registry errors).
# Digest logged for audit trail and reproducibility.
AGENT_PULL_RETRIES=3
AGENT_PULL_BACKOFF=5

pull_agent_image() {
  local tool="$1"
  local registry="$2"
  local remote_ref="$registry/agent-$tool:latest"

  for attempt in $(seq 1 "$AGENT_PULL_RETRIES"); do
    if docker pull "$remote_ref" 2>&1; then
      # Tag for platform service (agentforge-agent:<tool>) and compat alias
      docker tag "$remote_ref" "agentforge-agent:$tool"
      docker tag "agentforge-agent:$tool" "agentforge-agent-$tool:latest" 2>/dev/null || true

      # Log digest for audit trail
      local digest
      digest=$(docker inspect --format='{{index .RepoDigests 0}}' "$remote_ref" 2>/dev/null || echo "unknown")
      log "  $tool: pulled successfully (digest: $digest)"
      return 0
    fi

    if [ "$attempt" -lt "$AGENT_PULL_RETRIES" ]; then
      log "  $tool: attempt $attempt/$AGENT_PULL_RETRIES failed, retrying in ${AGENT_PULL_BACKOFF}s..."
      sleep "$AGENT_PULL_BACKOFF"
    fi
  done

  log "  $tool: all $AGENT_PULL_RETRIES pull attempts failed"
  return 1
}

if [ -n "${AGENT_REGISTRY:-}" ]; then
  log "Pulling agent images from registry ($AGENT_REGISTRY)..."
  UPDATED=0
  CLAUDE_OK=false

  for tool in claude opencode codex gemini; do
    log "  Pulling agent-$tool..."
    if pull_agent_image "$tool" "$AGENT_REGISTRY"; then
      UPDATED=$((UPDATED + 1))
      [ "$tool" = "claude" ] && CLAUDE_OK=true
    else
      if [ "$tool" = "claude" ]; then
        # Claude is required — check if a local image exists as last resort
        if docker image inspect "agentforge-agent:claude" >/dev/null 2>&1; then
          log "  claude: registry pull failed but local image exists — using local"
          CLAUDE_OK=true
        fi
      else
        log "  $tool: skipped (optional — existing local image used if available)"
      fi
    fi
  done

  # Backwards-compat alias: agentforge-agent:latest → claude
  docker tag "agentforge-agent:claude" "agentforge-agent:latest" 2>/dev/null || true

  log "Agent image pull complete: $UPDATED updated from registry"

  if [ "$CLAUDE_OK" = "false" ]; then
    log_error "FATAL: Claude agent image unavailable (not in registry, not local)"
    log_error "Agent creation will fail. Run 'make build-agent' or push image to registry."
    exit 1
  fi
else
  log "AGENT_REGISTRY not set — skipping agent image pull"
  # Pre-flight: verify claude image exists locally (required for agent creation)
  if ! docker image inspect "agentforge-agent:claude" >/dev/null 2>&1; then
    log_error "FATAL: agentforge-agent:claude not found and AGENT_REGISTRY not configured"
    log_error "Run 'make build-agent' or set AGENT_REGISTRY in docker/.env"
    exit 1
  fi
fi

# ---------------------------------------------------------------------------
# 3b. Pre-flight: verify bind mount source files exist
# ---------------------------------------------------------------------------
# Docker silently creates directories for missing bind mount sources,
# causing services to crash with confusing errors (e.g. "is a directory").
REQUIRED_FILES="nats.conf seccomp/agentforge-agent.json"
for f in $REQUIRED_FILES; do
  if [ ! -f "$f" ]; then
    log_error "Required config file missing: docker/$f"
    log_error "Ensure deploy bundle includes all bind mount sources"
    exit 1
  fi
done
log "Pre-flight check passed: all bind mount source files present"

# ---------------------------------------------------------------------------
# 4. Database migration (before new code starts)
# ---------------------------------------------------------------------------
# [I2] --no-deps is correct here: PostgreSQL is externally managed,
# not a compose service. We only need the agentforge container with new code
# to run migrations against the external database.
log "Running database migrations..."
# shellcheck disable=SC2086
docker compose $COMPOSE_FILES --profile external run --rm --no-deps agentforge-rust --migrate-only

# ---------------------------------------------------------------------------
# 5. Rolling restart with health check wait
# ---------------------------------------------------------------------------
# [B1] Phase 1: no auto-rollback. If startup fails, exit 1 for manual
# intervention. Auto-rollback is unsafe without schema rollback support.
log "Starting services..."
# shellcheck disable=SC2086
if ! docker compose $COMPOSE_FILES --profile external up -d \
  --remove-orphans --wait --wait-timeout 120; then
  log_error "Service startup failed! Manual intervention required."
  log_error "Check logs: docker compose $COMPOSE_FILES logs --tail=50"
  exit 1
fi

# ---------------------------------------------------------------------------
# 6. Atomic frontend deploy (symlink swap, zero 404 window)
# ---------------------------------------------------------------------------
log "Deploying frontend to $WEBROOT..."
RELEASE_DIR="$(dirname "$WEBROOT")/releases"
RELEASE_TS="$(date +%Y%m%d%H%M%S)"
RELEASE_PATH="$RELEASE_DIR/$RELEASE_TS"
TMP_DIST=$(mktemp -d)
FRONTEND_IMAGE="agentforge-frontend:${IMAGE_TAG:-latest}"
FRONTEND_CONTAINER=$(docker create "$FRONTEND_IMAGE")
trap 'docker rm -f "$FRONTEND_CONTAINER" >/dev/null 2>&1 || true; rm -rf "$TMP_DIST"' EXIT

if ! docker cp "$FRONTEND_CONTAINER":/app/dist/. "$TMP_DIST/"; then
  log_error "Failed to copy frontend files from image $FRONTEND_IMAGE"
  exit 1
fi

# Copy public assets (favicon, version.json, etc.) if they exist in image
docker cp "$FRONTEND_CONTAINER":/app/public/. "$TMP_DIST/" 2>/dev/null || true

# Ensure releases directory exists
mkdir -p "$RELEASE_DIR"

# Create versioned release directory and populate via container chown
docker run --rm \
  -v "$RELEASE_DIR":/releases \
  -v "$TMP_DIST":/src:ro \
  alpine:3.21 sh -c "
    mkdir -p /releases/$RELEASE_TS && \
    cp -r /src/* /releases/$RELEASE_TS/ && \
    chown -R 1000:1000 /releases/$RELEASE_TS
  "

# [B3] Handle first deploy: if WEBROOT is a directory (not symlink), back it up
if [ -d "$WEBROOT" ] && [ ! -L "$WEBROOT" ]; then
  log "First symlink deploy: migrating existing directory..."
  mv "$WEBROOT" "$RELEASE_DIR/pre-symlink-backup"
fi

# Atomic symlink swap — ln -sfn + mv -T is atomic on Linux (single rename(2) syscall)
ln -sfn "$RELEASE_PATH" "${WEBROOT}.tmp"
mv -T "${WEBROOT}.tmp" "$WEBROOT"

log "Frontend deployed (release: $RELEASE_TS). File count: $(find "$RELEASE_PATH" -type f | wc -l)"

# Cleanup: keep last 5 releases (+ pre-symlink-backup if it exists)
# shellcheck disable=SC2012
ls -1dt "$RELEASE_DIR"/*/  2>/dev/null | grep -v "pre-symlink-backup" | tail -n +6 | xargs rm -rf 2>/dev/null || true

# ---------------------------------------------------------------------------
# 7. Health check
# ---------------------------------------------------------------------------
log "Running health check..."
HEALTH_URL="http://localhost:${AGENTFORGE_PORT:-4003}/api/health"
HEALTH_OK=false
for i in $(seq 1 10); do
  if curl -sf --max-time 5 "$HEALTH_URL" >/dev/null 2>&1; then
    HEALTH_OK=true
    break
  fi
  log "Health check attempt $i/10 failed, retrying in 3s..."
  sleep 3
done

if [ "$HEALTH_OK" = "true" ]; then
  log "Health check passed!"
else
  # [B1] Phase 1: no auto-rollback, just fail loudly for manual intervention
  log_error "Health check failed after 10 attempts!"
  log_error "Service may be degraded. Manual intervention required."
  log_error "Check: curl -v $HEALTH_URL"
  exit 1
fi

# [I1] Print total deploy duration
log "Deploy complete in ${SECONDS}s"

#!/bin/bash
# =============================================================================
# Wisdoverse Forge — Single-Host Docker-Compose Deploy
# =============================================================================
# Reference deploy script for the docker-compose topology used by the project.
# Other deployment strategies (Kubernetes, Nomad, hosted Docker, plain rsync)
# are documented in `docs/deployment/single-host-compose.md`. Operators on
# different topologies should write their own deploy entry-point and reuse
# the validators (`validate-deploy-nats-env.sh`, `check-production-env.sh`).
#
# Usage: deploy.sh <staging|production> [image_tag] [registry_image]
#
# Runtime contract (required env, fail-fast):
#   WEBROOT                Absolute path nginx serves the SPA from.
#                          Production: required, no default.
#                          Staging: warns + falls back to /opt/agentforge/www
#                                   (legacy default; will be removed).
#
# Runtime contract (optional env, documented defaults):
#   AGENT_REGISTRY              Registry to pull agent images from. Unset = local-only.
#   AGENT_TOOLS                 Space-separated agent CLI list. Default: "claude opencode codex gemini".
#   REQUIRED_AGENT_TOOL         Must be present after pull or deploy fails. Default: "claude".
#   AGENTFORGE_NETWORKS         Space-separated docker networks to ensure exist.
#                               Default: "agentforge-agents external-network".
#   COMPOSE_FILES_OVERRIDE      Override `-f compose.yml -f compose.external.yml`.
#   COMPOSE_PROFILE             Compose profile passed via `--profile`. Default: "external".
#   COMPOSE_SERVICE_NAME        Service name for migrate/audit lookups. Default: "agentforge-server".
#   FRONTEND_IMAGE              Local docker image holding `/app/dist/`.
#                               Default: "agentforge-frontend:${IMAGE_TAG:-latest}".
#   FRONTEND_DEPLOY_MODE        "symlink" (default) or "rsync".
#                               symlink: atomic via release dirs + symlink swap. Requires nginx
#                                        to follow symlinks (no `disable_symlinks if_not_owner`).
#                               rsync:   keeps WEBROOT a real directory; non-atomic but tolerates
#                                        nginx configs that refuse symlinks.
#   WEBROOT_OWNER_UID           Owner UID for written files. Default: 1000.
#   WEBROOT_OWNER_GID           Owner GID for written files. Default: 1000.
#   KEEP_RELEASES               How many release dirs to retain in symlink mode. Default: 5.
#   AGENT_PULL_RETRIES          Retry attempts for agent pulls. Default: 3.
#   AGENT_PULL_BACKOFF          Seconds between retries. Default: 5.
#   HEALTH_PATH                 Health endpoint path. Default: "/api/health".
#   HEALTH_RETRIES              Curl retry count for health probe. Default: 10.
#   HEALTH_BACKOFF              Seconds between health retries. Default: 3.
#   AGENTFORGE_PORT             Port for health probe (`localhost:$PORT`). Default: 4003.
#   BUNDLE_REQUIRED_FILES       Newline- or space-separated bind-mount source files
#                               under `docker/`. Default: "nats.conf seccomp/agentforge-agent.json".
#   VERIFY_IMAGE_SIGNATURES     When `true`, runs `cosign verify` against every
#                               pulled image (server, orchestrator, frontend, and
#                               each agent overlay) before retagging it locally.
#                               Requires `cosign` on PATH and the GHCR publish
#                               workflow to attach SLSA provenance via
#                               `provenance: true` (already the case in
#                               `.github/workflows/publish-images.yml`).
#                               Recommended for production. Default: false.
#   COSIGN_CERT_IDENTITY_REGEX  Cert identity regex passed to cosign verify.
#                               Default: matches the project's GHCR publish
#                               workflow URL.
#   COSIGN_OIDC_ISSUER          OIDC issuer URL for cosign verify.
#                               Default: https://token.actions.githubusercontent.com
#
# Prerequisites assumed by this script (override or document in your fork):
#   - Single host (no clustering); operator handles fail-over manually.
#   - PostgreSQL is externally managed, not a compose service.
#   - Server has pre-configured `docker login` to the container registry.
#   - In symlink mode, nginx is configured to follow symlinks under WEBROOT.
#   - Manual rollback only (no auto-revert on health-check failure).
# =============================================================================

set -euo pipefail

ENV="${1:?Usage: deploy.sh <staging|production> [image_tag] [registry_image]}"
IMAGE_TAG="${2:-}"
REGISTRY_IMAGE="${3:-}"
DEPLOY_DIR="$(cd "$(dirname "$0")/.." && pwd)"
LOG_PREFIX="[deploy:$ENV]"

log() { echo "$(date '+%H:%M:%S') $LOG_PREFIX $*"; }
log_error() { echo "$(date '+%H:%M:%S') $LOG_PREFIX ERROR: $*" >&2; }

# verify_image_signature <fully-qualified-image-ref>
#   When VERIFY_IMAGE_SIGNATURES=true (recommended for production), validates
#   the Sigstore/cosign keyless signature attached by the GHCR publish workflow
#   (`provenance: true` on docker/build-push-action). Verification runs against
#   the project's published certificate identity regex and OIDC issuer; the
#   defaults match `.github/workflows/publish-images.yml`. Operators on a fork
#   can override COSIGN_CERT_IDENTITY_REGEX / COSIGN_OIDC_ISSUER per deploy.
#
#   Skips silently when the flag is unset/false to preserve backwards-compat.
#   Fails hard when the flag is true and either cosign is missing or the
#   signature does not verify, because a missing signature on a production
#   deploy is a deliberate decision rather than a graceful degradation.
verify_image_signature() {
  local image="$1"

  case "${VERIFY_IMAGE_SIGNATURES:-false}" in
    1 | true | yes | on) ;;
    *) return 0 ;;
  esac

  if ! command -v cosign >/dev/null 2>&1; then
    log_error "VERIFY_IMAGE_SIGNATURES=true but 'cosign' is not installed."
    log_error "Install: https://docs.sigstore.dev/cosign/system_config/installation/"
    return 1
  fi

  local cert_regex="${COSIGN_CERT_IDENTITY_REGEX:-https://github.com/Wisdoverse/Wisdoverse-Forge/.+}"
  local oidc_issuer="${COSIGN_OIDC_ISSUER:-https://token.actions.githubusercontent.com}"

  log "Verifying signature: $image"
  if ! cosign verify "$image" \
    --certificate-identity-regexp "$cert_regex" \
    --certificate-oidc-issuer "$oidc_issuer" \
    >/dev/null 2>&1; then
    log_error "Signature verification failed for $image"
    log_error "Run with VERIFY_IMAGE_SIGNATURES=false to bypass (not recommended)."
    return 1
  fi
}

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

# ---------------------------------------------------------------------------
# 0. Load environment config (docker/.env) — all environments
# ---------------------------------------------------------------------------
# docker/.env contains AGENT_REGISTRY, port overrides, and other deployment
# config. Load it as Docker Compose dotenv, not as shell code, because values
# such as `SMTP_FROM=IT <it@example.com>` are valid dotenv but invalid shell.
cd "$DEPLOY_DIR"
load_compose_env docker/.env

# Optional: NATS rollout-flag validator. Lives in its own script so non-NATS
# deployments can swap it out without editing this file.
if [ -x "scripts/validate-deploy-nats-env.sh" ]; then
  ./scripts/validate-deploy-nats-env.sh
fi

# ---------------------------------------------------------------------------
# Resolve deploy contract from env (required + optional with defaults)
# ---------------------------------------------------------------------------
COMPOSE_FILES="${COMPOSE_FILES_OVERRIDE:--f compose.yml -f compose.external.yml}"
COMPOSE_PROFILE="${COMPOSE_PROFILE:-external}"
COMPOSE_SERVICE_NAME="${COMPOSE_SERVICE_NAME:-agentforge-server}"
AGENT_TOOLS="${AGENT_TOOLS:-claude opencode codex gemini}"
REQUIRED_AGENT_TOOL="${REQUIRED_AGENT_TOOL:-claude}"
AGENTFORGE_NETWORKS="${AGENTFORGE_NETWORKS:-agentforge-agents external-network}"
WEBROOT_OWNER_UID="${WEBROOT_OWNER_UID:-1000}"
WEBROOT_OWNER_GID="${WEBROOT_OWNER_GID:-1000}"
KEEP_RELEASES="${KEEP_RELEASES:-5}"
AGENT_PULL_RETRIES="${AGENT_PULL_RETRIES:-3}"
AGENT_PULL_BACKOFF="${AGENT_PULL_BACKOFF:-5}"
HEALTH_PATH="${HEALTH_PATH:-/api/health}"
HEALTH_RETRIES="${HEALTH_RETRIES:-10}"
HEALTH_BACKOFF="${HEALTH_BACKOFF:-3}"
HEALTH_PORT="${AGENTFORGE_PORT:-4003}"
BUNDLE_REQUIRED_FILES="${BUNDLE_REQUIRED_FILES:-nats.conf seccomp/agentforge-agent.json}"
FRONTEND_DEPLOY_MODE="${FRONTEND_DEPLOY_MODE:-symlink}"
FRONTEND_IMAGE="${FRONTEND_IMAGE:-agentforge-frontend:${IMAGE_TAG:-latest}}"

case "$FRONTEND_DEPLOY_MODE" in
  symlink | rsync) ;;
  *)
    log_error "FRONTEND_DEPLOY_MODE must be 'symlink' or 'rsync', got: $FRONTEND_DEPLOY_MODE"
    exit 1
    ;;
esac

case "$ENV" in
  staging)
    if [ -z "${WEBROOT:-}" ]; then
      WEBROOT="/opt/agentforge/www"
      log "WARN: WEBROOT unset; using legacy default $WEBROOT."
      log "WARN: Set WEBROOT in docker/.env to the absolute path nginx serves from."
      log "WARN: This fallback will be removed; production already requires an explicit WEBROOT."
    fi
    ;;
  production)
    : "${WEBROOT:?WEBROOT environment variable must be set}"
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
# Use `docker compose ps -q` to find the container by service name —
# avoids hardcoding the container name (which carries a project prefix).
log "Recording current image digest for audit..."
CURRENT_CONTAINER=$(docker compose $COMPOSE_FILES ps -q "$COMPOSE_SERVICE_NAME" 2>/dev/null || echo "")
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
for net in $AGENTFORGE_NETWORKS; do
  docker network create "$net" 2>/dev/null || true
done

# ---------------------------------------------------------------------------
# 3. Pull images from registry (Build Once, Deploy Everywhere)
# ---------------------------------------------------------------------------
if [ -n "$IMAGE_TAG" ] && [ -n "$REGISTRY_IMAGE" ]; then
  log "Pulling images from registry (tag: $IMAGE_TAG)..."

  REMOTE_SERVER="$REGISTRY_IMAGE/server:$IMAGE_TAG"
  log "Pulling $REMOTE_SERVER..."
  if ! docker pull "$REMOTE_SERVER"; then
    log_error "Failed to pull $REMOTE_SERVER"
    exit 1
  fi
  if ! verify_image_signature "$REMOTE_SERVER"; then exit 1; fi
  docker tag "$REMOTE_SERVER" "agentforge-server:$IMAGE_TAG"
  docker tag "$REMOTE_SERVER" "agentforge-server:latest"

  REMOTE_ORCHESTRATOR="$REGISTRY_IMAGE/orchestrator:$IMAGE_TAG"
  log "Pulling $REMOTE_ORCHESTRATOR..."
  if ! docker pull "$REMOTE_ORCHESTRATOR"; then
    log_error "Failed to pull $REMOTE_ORCHESTRATOR"
    exit 1
  fi
  if ! verify_image_signature "$REMOTE_ORCHESTRATOR"; then exit 1; fi
  docker tag "$REMOTE_ORCHESTRATOR" "agentforge-orchestrator:$IMAGE_TAG"
  docker tag "$REMOTE_ORCHESTRATOR" "agentforge-orchestrator:latest"

  REMOTE_FRONTEND="$REGISTRY_IMAGE:$IMAGE_TAG"
  log "Pulling $REMOTE_FRONTEND..."
  if ! docker pull "$REMOTE_FRONTEND"; then
    log_error "Failed to pull $REMOTE_FRONTEND"
    exit 1
  fi
  if ! verify_image_signature "$REMOTE_FRONTEND"; then exit 1; fi
  docker tag "$REMOTE_FRONTEND" "agentforge-frontend:$IMAGE_TAG"
  docker tag "$REMOTE_FRONTEND" "agentforge-frontend:latest"

  log "Registry images pulled and tagged successfully"
else
  log "No registry image specified — using locally available images"
fi

# ---------------------------------------------------------------------------
# 3a. Pull agent images from registry
# ---------------------------------------------------------------------------
# REQUIRED_AGENT_TOOL must be present after pull or deploy fails. Other tools
# in AGENT_TOOLS are best-effort: existing local images are used as fallback.
# Retries handle transient registry errors; digests are logged for audit.

pull_agent_image() {
  local tool="$1"
  local registry="$2"
  local remote_ref="$registry/agent-$tool:latest"

  for attempt in $(seq 1 "$AGENT_PULL_RETRIES"); do
    if docker pull "$remote_ref" 2>&1; then
      if ! verify_image_signature "$remote_ref"; then
        return 1
      fi
      docker tag "$remote_ref" "agentforge-agent:$tool"
      docker tag "agentforge-agent:$tool" "agentforge-agent-$tool:latest" 2>/dev/null || true

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
  REQUIRED_OK=false

  for tool in $AGENT_TOOLS; do
    log "  Pulling agent-$tool..."
    if pull_agent_image "$tool" "$AGENT_REGISTRY"; then
      UPDATED=$((UPDATED + 1))
      [ "$tool" = "$REQUIRED_AGENT_TOOL" ] && REQUIRED_OK=true
    else
      if [ "$tool" = "$REQUIRED_AGENT_TOOL" ]; then
        if docker image inspect "agentforge-agent:$REQUIRED_AGENT_TOOL" >/dev/null 2>&1; then
          log "  $tool: registry pull failed but local image exists — using local"
          REQUIRED_OK=true
        fi
      else
        log "  $tool: skipped (optional — existing local image used if available)"
      fi
    fi
  done

  # Backwards-compat alias: agentforge-agent:latest → required tool.
  docker tag "agentforge-agent:$REQUIRED_AGENT_TOOL" "agentforge-agent:latest" 2>/dev/null || true

  log "Agent image pull complete: $UPDATED updated from registry"

  if [ "$REQUIRED_OK" = "false" ]; then
    log_error "FATAL: required agent image '$REQUIRED_AGENT_TOOL' unavailable (not in registry, not local)"
    log_error "Agent creation will fail. Run 'make build-agent' or push the image to the registry."
    exit 1
  fi
else
  log "AGENT_REGISTRY not set — skipping agent image pull"
  if ! docker image inspect "agentforge-agent:$REQUIRED_AGENT_TOOL" >/dev/null 2>&1; then
    log_error "FATAL: agentforge-agent:$REQUIRED_AGENT_TOOL not found and AGENT_REGISTRY not configured"
    log_error "Run 'make build-agent' or set AGENT_REGISTRY in docker/.env"
    exit 1
  fi
fi

# ---------------------------------------------------------------------------
# 3b. Pre-flight: verify bind mount source files exist
# ---------------------------------------------------------------------------
# Docker silently creates directories for missing bind mount sources, causing
# services to crash with confusing errors (e.g. "is a directory").
for f in $BUNDLE_REQUIRED_FILES; do
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
# `--no-deps` is correct here: PostgreSQL is externally managed, not a compose
# service. We only need the agentforge container with new code to run
# migrations against the external database.
log "Running database migrations..."
# shellcheck disable=SC2086
docker compose $COMPOSE_FILES --profile "$COMPOSE_PROFILE" run --rm --no-deps "$COMPOSE_SERVICE_NAME" --migrate-only

# ---------------------------------------------------------------------------
# 5. Rolling restart with health check wait
# ---------------------------------------------------------------------------
# No auto-rollback. If startup fails, exit 1 for manual intervention. Auto-
# rollback is unsafe without paired schema rollback support.
log "Starting services..."
# shellcheck disable=SC2086
if ! docker compose $COMPOSE_FILES --profile "$COMPOSE_PROFILE" up -d \
  --remove-orphans --wait --wait-timeout 120; then
  log_error "Service startup failed! Manual intervention required."
  log_error "Check logs: docker compose $COMPOSE_FILES logs --tail=50"
  exit 1
fi

# ---------------------------------------------------------------------------
# 6. Frontend deploy
# ---------------------------------------------------------------------------
log "Deploying frontend to $WEBROOT (mode: $FRONTEND_DEPLOY_MODE)..."

TMP_DIST=$(mktemp -d)
FRONTEND_CONTAINER=$(docker create "$FRONTEND_IMAGE")
trap 'docker rm -f "$FRONTEND_CONTAINER" >/dev/null 2>&1 || true; rm -rf "$TMP_DIST"' EXIT

if ! docker cp "$FRONTEND_CONTAINER":/app/dist/. "$TMP_DIST/"; then
  log_error "Failed to copy frontend files from image $FRONTEND_IMAGE"
  exit 1
fi

# Public assets (favicon, version.json, etc.) live under /app/public if present.
docker cp "$FRONTEND_CONTAINER":/app/public/. "$TMP_DIST/" 2>/dev/null || true

if [ "$FRONTEND_DEPLOY_MODE" = "symlink" ]; then
  RELEASE_DIR="$(dirname "$WEBROOT")/releases"
  RELEASE_TS="$(date +%Y%m%d%H%M%S)"
  RELEASE_PATH="$RELEASE_DIR/$RELEASE_TS"

  mkdir -p "$RELEASE_DIR"

  # Populate release dir as the configured owner so nginx (and any FTP/file
  # tools sharing the directory) can read without ad-hoc chown later.
  docker run --rm \
    -v "$RELEASE_DIR":/releases \
    -v "$TMP_DIST":/src:ro \
    alpine:3.21 sh -c "
      mkdir -p /releases/$RELEASE_TS && \
      cp -r /src/* /releases/$RELEASE_TS/ && \
      chown -R $WEBROOT_OWNER_UID:$WEBROOT_OWNER_GID /releases/$RELEASE_TS
    "

  # First-deploy migration: if WEBROOT is a real directory, back it up so the
  # symlink swap below has somewhere to move the existing content.
  if [ -d "$WEBROOT" ] && [ ! -L "$WEBROOT" ]; then
    log "First symlink deploy: migrating existing directory to $RELEASE_DIR/pre-symlink-backup"
    mv "$WEBROOT" "$RELEASE_DIR/pre-symlink-backup"
  fi

  # Atomic symlink swap — `ln -sfn` + `mv -T` is a single rename(2) syscall.
  ln -sfn "$RELEASE_PATH" "${WEBROOT}.tmp"
  mv -T "${WEBROOT}.tmp" "$WEBROOT"

  log "Frontend deployed (release: $RELEASE_TS). File count: $(find "$RELEASE_PATH" -type f | wc -l)"

  # Retain the most recent $KEEP_RELEASES dirs plus pre-symlink-backup.
  # shellcheck disable=SC2012
  ls -1dt "$RELEASE_DIR"/*/ 2>/dev/null \
    | grep -v "pre-symlink-backup" \
    | tail -n +"$((KEEP_RELEASES + 1))" \
    | xargs rm -rf 2>/dev/null || true
else
  # rsync mode — overwrite a real directory in place. Non-atomic (browsers
  # mid-load may see an inconsistent set of assets for a few hundred ms),
  # but tolerates `disable_symlinks if_not_owner` and other strict nginx
  # configs that refuse to follow symlinks.
  if [ ! -d "$WEBROOT" ]; then
    log_error "rsync mode requires WEBROOT to be an existing directory: $WEBROOT"
    exit 1
  fi

  docker run --rm \
    -v "$WEBROOT":/dst \
    -v "$TMP_DIST":/src:ro \
    alpine:3.21 sh -c "
      apk add --no-cache --quiet rsync >/dev/null && \
      rsync -a --delete /src/ /dst/ && \
      chown -R $WEBROOT_OWNER_UID:$WEBROOT_OWNER_GID /dst
    "

  log "Frontend rsync'd into $WEBROOT. File count: $(find "$WEBROOT" -type f | wc -l)"
fi

# ---------------------------------------------------------------------------
# 7. Health check
# ---------------------------------------------------------------------------
log "Running health check..."
HEALTH_URL="http://localhost:${HEALTH_PORT}${HEALTH_PATH}"
HEALTH_OK=false
for i in $(seq 1 "$HEALTH_RETRIES"); do
  if curl -sf --max-time 5 "$HEALTH_URL" >/dev/null 2>&1; then
    HEALTH_OK=true
    break
  fi
  log "Health check attempt $i/$HEALTH_RETRIES failed, retrying in ${HEALTH_BACKOFF}s..."
  sleep "$HEALTH_BACKOFF"
done

if [ "$HEALTH_OK" = "true" ]; then
  log "Health check passed!"
else
  log_error "Health check failed after $HEALTH_RETRIES attempts!"
  log_error "Service may be degraded. Manual intervention required."
  log_error "Check: curl -v $HEALTH_URL"
  exit 1
fi

log "Deploy complete in ${SECONDS}s"

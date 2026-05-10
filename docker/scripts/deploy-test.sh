#!/bin/bash
# =============================================================================
# Wisdoverse Forge Deployment & Testing Script
# =============================================================================
#
# Complete CI/CD workflow: Build → Deploy → Test → Verify
#
# Usage:
#   ./docker/scripts/deploy-test.sh [mode]
#
# Modes:
#   dev      - Development environment (default)
#   staging  - Staging with external DB/Redis
#   prod     - Production deployment
#
# =============================================================================

set -euo pipefail

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Configuration
MODE="${1:-dev}"
COMPOSE_PROJECT="agentforge"
HEALTH_CHECK_RETRIES=30
HEALTH_CHECK_INTERVAL=2

# Validate mode
case "$MODE" in
  dev|staging|prod-ext|prod)
    ;;
  *)
    echo "Error: Unknown mode '$MODE'. Valid modes: dev, staging, prod-ext, prod"
    exit 1
    ;;
esac

log_info() { echo -e "${BLUE}[INFO]${NC} $1"; }
log_success() { echo -e "${GREEN}[SUCCESS]${NC} $1"; }
log_warning() { echo -e "${YELLOW}[WARNING]${NC} $1"; }
log_error() { echo -e "${RED}[ERROR]${NC} $1"; }

# =============================================================================
# Step 1: Pre-deployment Checks
# =============================================================================
pre_checks() {
  log_info "Running pre-deployment checks..."

  # Check Docker
  if ! command -v docker &> /dev/null; then
    log_error "Docker is not installed"
    exit 1
  fi

  # Check Docker Compose
  if ! docker compose version &> /dev/null; then
    log_error "Docker Compose is not available"
    exit 1
  fi

  # Check .env file
  if [ ! -f "docker/.env" ]; then
    log_warning ".env file not found, copying from .env.example"
    cp docker/.env.example docker/.env
    log_warning "Please configure docker/.env before production deployment"
  fi

  log_success "Pre-checks passed"
}

# =============================================================================
# Step 2: Build
# =============================================================================
build() {
  log_info "Building Docker images..."

  # Ensure external networks exist
  docker network create external-network 2>/dev/null || true
  docker network create agentforge-agents 2>/dev/null || true

  case "$MODE" in
    dev)
      docker compose -f docker/compose.yml -f docker/compose.dev.yml --profile dev build
      ;;
    staging|prod-ext)
      docker compose -f docker/compose.yml --profile external build
      ;;
    prod)
      docker compose -f docker/compose.yml -f docker/compose.prod.yml --profile prod build
      ;;
  esac

  log_success "Build completed"
}

# =============================================================================
# Step 3: Deploy
# =============================================================================
deploy() {
  log_info "Deploying in $MODE mode..."

  # Stop existing containers
  docker compose -p "$COMPOSE_PROJECT" down --remove-orphans 2>/dev/null || true

  case "$MODE" in
    dev)
      docker compose -f docker/compose.yml -f docker/compose.dev.yml --profile dev up -d
      ;;
    staging|prod-ext)
      docker compose -f docker/compose.yml --profile external up -d
      ;;
    prod)
      docker compose -f docker/compose.yml -f docker/compose.prod.yml --profile prod up -d
      ;;
  esac

  log_success "Deployment started"
}

# =============================================================================
# Step 4: Health Check
# =============================================================================
health_check() {
  log_info "Waiting for services to be healthy..."

  local server_url="http://localhost:${AGENTFORGE_PORT:-4003}"
  local retries=0

  while [ "$retries" -lt "$HEALTH_CHECK_RETRIES" ]; do
    if curl -sf "${server_url}/health" > /dev/null 2>&1; then
      log_success "Server is healthy"
      return 0
    fi

    retries=$((retries + 1))
    echo -n "."
    sleep "$HEALTH_CHECK_INTERVAL"
  done

  echo ""
  log_error "Health check failed after ${HEALTH_CHECK_RETRIES} retries"
  docker compose -p "$COMPOSE_PROJECT" logs --tail=50
  return 1
}

# =============================================================================
# Step 5: Run Tests
# =============================================================================
run_tests() {
  log_info "Running deployment verification tests..."

  local server_url="http://localhost:${AGENTFORGE_PORT:-4003}"
  local failed=0

  # Test 1: Health endpoint
  log_info "Testing /health endpoint..."
  if curl -sf "${server_url}/health" | grep -q '"ok":true'; then
    log_success "Health check passed"
  else
    log_error "Health check failed"
    failed=$((failed + 1))
  fi

  # Test 2: Version endpoint
  log_info "Testing /version endpoint..."
  if curl -sf "${server_url}/version" | grep -q '"version"'; then
    log_success "Version endpoint passed"
  else
    log_error "Version endpoint failed"
    failed=$((failed + 1))
  fi

  # Test 3: WebSocket connectivity (basic check)
  log_info "Testing WebSocket availability..."
  if curl -sf -o /dev/null -w "%{http_code}" "${server_url}/ws" 2>/dev/null | grep -q "400\|426"; then
    log_success "WebSocket endpoint available"
  else
    log_warning "WebSocket test inconclusive (may require upgrade)"
  fi

  # Test 4: LLM Provider API (if enabled)
  log_info "Testing LLM Provider API..."
  local llm_response=$(curl -sf "${server_url}/api/v1/llm-providers/supported" 2>/dev/null)
  if echo "$llm_response" | grep -q '"providers"'; then
    log_success "LLM Provider API passed"
    echo "  Supported providers: $llm_response"
  else
    log_warning "LLM Provider API not available (may require auth)"
  fi

  # Test 5: Database connectivity (via health)
  log_info "Testing database connectivity..."
  local health_detail=$(curl -sf "${server_url}/health" 2>/dev/null)
  if echo "$health_detail" | grep -q '"database"'; then
    log_success "Database connection verified"
  else
    log_warning "Database health not reported"
  fi

  # Summary
  echo ""
  if [ "$failed" -eq 0 ]; then
    log_success "All deployment tests passed!"
    return 0
  else
    log_error "$failed test(s) failed"
    return 1
  fi
}

# =============================================================================
# Step 6: Show Status
# =============================================================================
show_status() {
  log_info "Deployment Status:"
  echo ""
  docker compose -p "$COMPOSE_PROJECT" ps
  echo ""

  log_info "Service URLs:"
  echo "  - Server:    http://localhost:${AGENTFORGE_PORT:-4003}"
  echo "  - Health:    http://localhost:${AGENTFORGE_PORT:-4003}/health"
  echo "  - WebSocket: ws://localhost:${AGENTFORGE_PORT:-4003}/ws"
  echo "  - API:       http://localhost:${AGENTFORGE_PORT:-4003}/api/v1"
  echo ""

  log_info "Useful commands:"
  echo "  - View logs:     docker compose -p ${COMPOSE_PROJECT} logs -f"
  echo "  - Stop:          docker compose -p ${COMPOSE_PROJECT} down"
  echo "  - Restart:       docker compose -p ${COMPOSE_PROJECT} restart"
  echo "  - Run migration: docker compose -p ${COMPOSE_PROJECT} exec agentforge-server agentforge-server --migrate-only"
}

# =============================================================================
# Main
# =============================================================================
main() {
  echo ""
  echo "=============================================="
  echo "  Wisdoverse Forge Deploy & Test Pipeline"
  echo "  Mode: $MODE"
  echo "=============================================="
  echo ""

  pre_checks
  build
  deploy
  health_check
  run_tests
  show_status

  echo ""
  log_success "Deployment completed successfully!"
}

# Run
main "$@"

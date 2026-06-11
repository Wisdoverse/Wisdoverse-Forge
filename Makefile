# =============================================================================
# Wisdoverse Forge Makefile
# =============================================================================
#
# Profile-based Docker commands for development and production.
# All services defined in docker/compose.yml; profiles select what runs.
#
# Usage:
#   make setup        # One-time: create external Docker networks
#   make dev          # Development with Rust backend
#   make prod         # Production self-contained (DB, Redis, Caddy)
#   make prod-pull    # Production self-contained from GHCR images
#   make prod-ext     # Production with external services
#
# =============================================================================

.DEFAULT_GOAL := help

# Docker compose base command. Bootstrap targets create docker/.env; Compose must
# read that file explicitly because it otherwise only auto-loads .env from the
# current working directory.
COMPOSE_ENV_FILE ?= docker/.env
COMPOSE := docker compose --env-file $(COMPOSE_ENV_FILE) -f docker/compose.yml
SELFHOST_ENV := $(if $(HTTP_PORT),HTTP_PORT="$(HTTP_PORT)") $(if $(HTTPS_PORT),HTTPS_PORT="$(HTTPS_PORT)")

# China mirror support: loads .env.local if present (create via: cp .env.example.cn .env.local)
-include .env.local

# Build-args auto-constructed from .env.local variables (empty when not set)
DOCKER_CN_ARGS := $(if $(NPM_REGISTRY),--build-arg NPM_REGISTRY=$(NPM_REGISTRY)) \
                  $(if $(DOCKER_MIRROR),--build-arg DOCKER_MIRROR=$(DOCKER_MIRROR)) \
                  $(if $(GITHUB_PROXY),--build-arg GITHUB_PROXY=$(GITHUB_PROXY))

# External deploys read EXTERNAL_NETWORK from docker/.env via Compose. Mirror that
# here so setup-external creates the same network name when it is user-managed.
EXTERNAL_NETWORK_NAME ?= $(shell sh -c 'if [ -n "$$EXTERNAL_NETWORK" ]; then printf "%s" "$$EXTERNAL_NETWORK"; elif [ -f docker/.env ]; then sed -n "s/^EXTERNAL_NETWORK=//p" docker/.env | tail -n 1 | tr -d "\r"; fi')
OAUTH_MOUNT_DIR_NAME ?= $(shell sh -c 'if [ -n "$$OAUTH_MOUNT_DIR" ]; then printf "%s" "$$OAUTH_MOUNT_DIR"; elif [ -f docker/.env ]; then val=$$(sed -n "s/^OAUTH_MOUNT_DIR=//p" docker/.env | tail -n 1 | tr -d "\r"); if [ -n "$$val" ]; then printf "%s" "$$val"; else printf "/tmp/agentforge/oauth-mounts"; fi; else printf "/tmp/agentforge/oauth-mounts"; fi')
OAUTH_MOUNT_UID ?= 100
OAUTH_MOUNT_GID ?= 101

# =============================================================================
# Setup (one-time)
# =============================================================================

.PHONY: bootstrap-local
bootstrap-local: ## Prepare local docker/.env and prerequisite checks
	@bash scripts/bootstrap-local.sh

.PHONY: quickstart-local
quickstart-local: ## Prepare local env, start backend stack, and wait for health
	@bash scripts/bootstrap-local.sh --start
	@bash scripts/check-local-runtime.sh --wait

.PHONY: local-health
local-health: ## Check local backend runtime health
	@bash scripts/check-local-runtime.sh --wait

.PHONY: bootstrap-selfhost
bootstrap-selfhost: ## Prepare self-contained production profile
	@$(SELFHOST_ENV) bash scripts/bootstrap-selfhost.sh $(if $(DOMAIN),--domain "$(DOMAIN)")

.PHONY: selfhost-check
selfhost-check: ## Validate self-contained production prerequisites
	@$(SELFHOST_ENV) bash scripts/bootstrap-selfhost.sh --check $(if $(DOMAIN),--domain "$(DOMAIN)")

.PHONY: selfhost-health
selfhost-health: ## Check self-contained production public ingress health
	@$(SELFHOST_ENV) bash scripts/check-selfhost-runtime.sh --wait $(if $(DOMAIN),--domain "$(DOMAIN)")

.PHONY: beginner-audit
beginner-audit: ## Audit beginner self-host readiness
	@$(SELFHOST_ENV) $(if $(DOMAIN),DOMAIN="$(DOMAIN)") bash scripts/audit-beginner-selfhost.sh $(BEGINNER_AUDIT_FLAGS)

.PHONY: quickstart-selfhost
quickstart-selfhost: setup ## Prepare, start, and verify self-contained production
	@$(SELFHOST_ENV) bash scripts/bootstrap-selfhost.sh $(if $(DOMAIN),--domain "$(DOMAIN)")
	$(SELFHOST_ENV) $(COMPOSE) -f docker/compose.prod.yml --profile prod up -d --build
	@$(SELFHOST_ENV) bash scripts/check-selfhost-runtime.sh --wait $(if $(DOMAIN),--domain "$(DOMAIN)")

.PHONY: quickstart-selfhost-pull
quickstart-selfhost-pull: setup ## Prepare, pull GHCR images, start, and verify self-contained production
	@$(SELFHOST_ENV) bash scripts/bootstrap-selfhost.sh $(if $(DOMAIN),--domain "$(DOMAIN)")
	@$(MAKE) pull-server-images update-agents
	$(SELFHOST_ENV) $(COMPOSE) -f docker/compose.prod.yml --profile prod up -d --remove-orphans
	@$(SELFHOST_ENV) bash scripts/check-selfhost-runtime.sh --wait $(if $(DOMAIN),--domain "$(DOMAIN)")

.PHONY: setup
setup: ## Ensure external Docker networks exist
	@docker network create agentforge-agents 2>/dev/null || true
	@mkdir -p "$(OAUTH_MOUNT_DIR_NAME)"
	@docker run --rm --user 0:0 -v "$(OAUTH_MOUNT_DIR_NAME):/oauth-mount-dir" alpine:3.21 sh -c 'chown $(OAUTH_MOUNT_UID):$(OAUTH_MOUNT_GID) /oauth-mount-dir && chmod 700 /oauth-mount-dir' >/dev/null

.PHONY: setup-external
setup-external: setup ## Create networks for external profile
	@docker network create $(if $(EXTERNAL_NETWORK_NAME),$(EXTERNAL_NETWORK_NAME),external-network) 2>/dev/null || true

# =============================================================================
# Development
# =============================================================================

.PHONY: dev
dev: setup ## Start development environment with Rust backend
	$(COMPOSE) -f docker/compose.dev.yml --profile dev up --build

.PHONY: dev-d
dev-d: setup ## Start development environment (detached)
	$(COMPOSE) -f docker/compose.dev.yml --profile dev up -d --build

.PHONY: dev-tools
dev-tools: setup ## Start development with admin tools (Adminer, Redis Commander)
	$(COMPOSE) -f docker/compose.dev.yml --profile dev --profile tools up --build

.PHONY: dev-casdoor
dev-casdoor: setup ## Start development with Casdoor SSO
	$(COMPOSE) -f docker/compose.dev.yml --profile dev --profile casdoor up --build

.PHONY: dev-down
dev-down: ## Stop development environment
	$(COMPOSE) -f docker/compose.dev.yml --profile dev --profile tools down

.PHONY: dev-logs
dev-logs: ## View development logs
	$(COMPOSE) -f docker/compose.dev.yml --profile dev logs -f

# =============================================================================
# Production — Self-Contained (DB + Redis + Caddy)
# =============================================================================

.PHONY: prod
prod: setup ## Start production with full stack (DB, Redis, Caddy)
	$(COMPOSE) -f docker/compose.prod.yml --profile prod up -d --build

.PHONY: prod-pull
prod-pull: setup pull-server-images update-agents ## Start production with full stack using GHCR images
	$(COMPOSE) -f docker/compose.prod.yml --profile prod up -d --remove-orphans

.PHONY: prod-backup
prod-backup: setup ## Start production with backup service
	$(COMPOSE) -f docker/compose.prod.yml --profile prod --profile backup up -d --build

.PHONY: prod-storage
prod-storage: setup ## Start production with MinIO object storage
	$(COMPOSE) -f docker/compose.prod.yml --profile prod --profile storage up -d --build

.PHONY: prod-casdoor
prod-casdoor: setup ## Start production with Casdoor SSO
	$(COMPOSE) -f docker/compose.prod.yml --profile prod --profile casdoor up -d --build

.PHONY: prod-down
prod-down: ## Stop production full stack
	$(COMPOSE) -f docker/compose.prod.yml --profile prod down

.PHONY: prod-logs
prod-logs: ## View production logs
	$(COMPOSE) -f docker/compose.prod.yml --profile prod logs -f

# =============================================================================
# Production — External Services (external DB/Redis)
# =============================================================================

.PHONY: prod-ext
prod-ext: setup-external ## Start production with external services
	COMPOSE_PARALLEL_LIMIT=1 $(COMPOSE) -f docker/compose.external.yml --profile external up -d --build --remove-orphans

.PHONY: prod-ext-down
prod-ext-down: ## Stop production with external services
	$(COMPOSE) -f docker/compose.external.yml --profile external down

.PHONY: prod-ext-logs
prod-ext-logs: ## View production external logs
	$(COMPOSE) -f docker/compose.external.yml --profile external logs -f

# =============================================================================
# Production from GHCR (no local build of Rust / frontend)
# =============================================================================
#
# Pulls server images from GHCR and rebuilds only the Claude agent image
# locally. Claude is intentionally not published in public GHCR because its
# package license points to Anthropic terms rather than a standard
# open-source redistribution license — operators must build it themselves
# after accepting those terms (see docs/deployment/single-host-compose.md).
#
# GHCR_IMAGE_TAG follows the same scheme as scripts/deploy.sh:
#   * Set to `main` to track the latest main-branch GHCR build (default).
#   * Pin to `:0.2.0`, `:sha-abc1234`, `:edge`, etc. for reproducibility.
#
# Override the registry namespace via PUBLIC_AGENT_REGISTRY (defaults to
# `ghcr.io/wisdoverse/wisdoverse-forge`) when forking the project.

GHCR_IMAGE_TAG ?= $(or $(shell grep -m1 '^GHCR_IMAGE_TAG=' docker/.env 2>/dev/null | cut -d= -f2),main)

.PHONY: pull-server-images
pull-server-images: ## Pull server / orchestrator / frontend from GHCR and re-tag for compose
	@if [ -z "$(AGENT_REGISTRY)" ]; then \
		echo "Error: AGENT_REGISTRY not set."; \
		echo "Set it in docker/.env or pass via: make pull-server-images AGENT_REGISTRY=$(PUBLIC_AGENT_REGISTRY)"; \
		exit 1; \
	fi
	@echo "Pulling server images from $(AGENT_REGISTRY) (tag: $(GHCR_IMAGE_TAG))..."
	docker pull $(AGENT_REGISTRY)/server:$(GHCR_IMAGE_TAG)
	docker tag $(AGENT_REGISTRY)/server:$(GHCR_IMAGE_TAG) agentforge-server:$(GHCR_IMAGE_TAG)
	docker tag $(AGENT_REGISTRY)/server:$(GHCR_IMAGE_TAG) agentforge-server:latest
	docker pull $(AGENT_REGISTRY)/orchestrator:$(GHCR_IMAGE_TAG)
	docker tag $(AGENT_REGISTRY)/orchestrator:$(GHCR_IMAGE_TAG) agentforge-orchestrator:$(GHCR_IMAGE_TAG)
	docker tag $(AGENT_REGISTRY)/orchestrator:$(GHCR_IMAGE_TAG) agentforge-orchestrator:latest
	docker pull $(AGENT_REGISTRY):$(GHCR_IMAGE_TAG)
	docker tag $(AGENT_REGISTRY):$(GHCR_IMAGE_TAG) agentforge-frontend:$(GHCR_IMAGE_TAG)
	docker tag $(AGENT_REGISTRY):$(GHCR_IMAGE_TAG) agentforge-frontend:latest
	@echo "Server images pulled and tagged for compose."

.PHONY: prod-ext-pull
prod-ext-pull: setup-external pull-server-images update-agent-base build-claude ## Production with external services using GHCR images (Claude built locally)
	@echo "Bringing up the stack without --build (uses pulled images)..."
	$(COMPOSE) -f docker/compose.external.yml --profile external up -d --remove-orphans

.PHONY: build-claude
build-claude: ensure-agent-base ## Build the Claude agent image locally (license requires self-build)
	@$(MAKE) build-agent CLI_TOOL=claude

# =============================================================================
# Rust Backend (replaces TS+Go with single Rust binary)
# =============================================================================

.PHONY: rust-ext
rust-ext: setup-external ## Start Rust backend with external services
	$(COMPOSE) -f docker/compose.external.yml --profile rust --profile external up -d agentforge-server nats --build

.PHONY: rust-ext-down
rust-ext-down: ## Stop Rust backend
	$(COMPOSE) --profile rust down --remove-orphans

.PHONY: rust-ext-logs
rust-ext-logs: ## View Rust backend logs
	docker logs -f agentforge-server

# =============================================================================
# Staging — your-staging-domain.com (external services)
# =============================================================================

.PHONY: staging
staging: setup-external ## Start staging environment (external services)
	$(COMPOSE) -f docker/compose.external.yml --profile external up -d --build --remove-orphans

.PHONY: staging-down
staging-down: ## Stop staging environment
	$(COMPOSE) -f docker/compose.external.yml --profile external down

.PHONY: staging-logs
staging-logs: ## View staging logs
	$(COMPOSE) -f docker/compose.external.yml --profile external logs -f

.PHONY: deploy-staging
deploy-staging: ## Deploy to staging via deploy script
	bash scripts/deploy.sh staging

.PHONY: deploy-production
deploy-production: ## Deploy to production via deploy script
	bash scripts/deploy.sh production

# =============================================================================
# Database
# =============================================================================

.PHONY: migrate
migrate: ## Run database migrations
	docker exec agentforge-server agentforge-server --migrate-only

.PHONY: migrate-status
migrate-status: ## Check migration status
	@echo "Migration status is not exposed by the Rust CLI. Inspect _sqlx_migrations in PostgreSQL." >&2
	@exit 1

# =============================================================================
# Build
# =============================================================================

.PHONY: build
build: ## Build production image
	docker build $(DOCKER_CN_ARGS) -t agentforge:latest -f docker/Dockerfile --target production .

.PHONY: build-dev
build-dev: ## Build development image
	docker build $(DOCKER_CN_ARGS) -t agentforge:dev -f docker/Dockerfile --target development .

.PHONY: build-no-cache
build-no-cache: ## Build production image without cache
	docker build $(DOCKER_CN_ARGS) -t agentforge:latest -f docker/Dockerfile --target production --no-cache .

.PHONY: build-agent-base
build-agent-base: ## Build agent base image (system deps, sidecar, platform CLIs)
	$(eval _UID := $(or $(CLAUDE_UID),$(shell grep -m1 '^CLAUDE_UID=' docker/.env 2>/dev/null | cut -d= -f2),1011))
	$(eval _GID := $(or $(CLAUDE_GID),$(shell grep -m1 '^CLAUDE_GID=' docker/.env 2>/dev/null | cut -d= -f2),1012))
	docker build $(DOCKER_CN_ARGS) -t agentforge-agent-base:latest \
		-f docker/Dockerfile.agent-base \
		--build-arg AGENT_UID=$(_UID) \
		--build-arg AGENT_GID=$(_GID) .

.PHONY: ensure-agent-base
ensure-agent-base: ## Ensure base image exists (pull or build)
	@docker image inspect agentforge-agent-base:latest >/dev/null 2>&1 \
		|| { echo "Base image not found locally, building..."; $(MAKE) build-agent-base; }

.PHONY: build-agent
build-agent: ensure-agent-base ## Build single CLI agent image (default: claude)
	$(eval _TOOL := $(or $(CLI_TOOL),claude))
	$(eval _PKG := $(shell echo "claude:@anthropic-ai/claude-code opencode:opencode-ai codex:@openai/codex gemini:@google/gemini-cli" | tr ' ' '\n' | grep "^$(_TOOL):" | cut -d: -f2))
	$(eval _VER := $(or $(CLI_VERSION),$(shell npm view "$(_PKG)" version 2>/dev/null || echo "latest")))
	docker build $(DOCKER_CN_ARGS) -t agentforge-agent:$(_TOOL) -t agentforge-agent:$(_TOOL)-$(_VER) \
		-f docker/Dockerfile.agent \
		--build-arg CLI_TOOL=$(_TOOL) \
		--build-arg CLI_VERSION=$(_VER) \
		--label org.wisdoverse.cli-version=$(_VER) \
		--label org.agentforge.cli-version=$(_VER) .
	@docker tag agentforge-agent:$(_TOOL) agentforge-agent-$(_TOOL):latest || \
		echo "WARNING: compat tag agentforge-agent-$(_TOOL):latest failed"
	@if [ "$(_TOOL)" = "claude" ]; then docker tag agentforge-agent:claude agentforge-agent:latest || \
		echo "WARNING: agentforge-agent:latest alias failed"; fi

ALL_AGENT_TOOLS ?= claude opencode codex gemini
LOCAL_AGENT_TOOLS ?= $(ALL_AGENT_TOOLS)

.PHONY: build-agent-all
build-agent-all: ensure-agent-base ## Build agent images for all CLI tools with latest npm versions
	@for tool in $(LOCAL_AGENT_TOOLS); do \
		PKG=$$(echo "claude:@anthropic-ai/claude-code opencode:opencode-ai codex:@openai/codex gemini:@google/gemini-cli" | tr ' ' '\n' | grep "^$${tool}:" | cut -d: -f2); \
		VER=$$(npm view "$$PKG" version 2>/dev/null || echo "latest"); \
		echo "=== Building agent-$$tool@$$VER ==="; \
		docker build $(DOCKER_CN_ARGS) -t agentforge-agent:$$tool -t agentforge-agent:$$tool-$$VER \
			-f docker/Dockerfile.agent \
			--build-arg CLI_TOOL=$$tool \
			--build-arg CLI_VERSION=$$VER \
			--label org.wisdoverse.cli-version=$$VER .; \
		docker tag agentforge-agent:$$tool agentforge-agent-$$tool:latest 2>/dev/null || true; \
	done
	@docker tag agentforge-agent:claude agentforge-agent:latest 2>/dev/null || true
	@$(MAKE) sync-env

# =============================================================================
# Agent Image Updates (pull latest from registry)
# =============================================================================

# Defaults to the public GitHub Container Registry namespace used by releases.
# Private deployments can override in docker/.env or on the command line, e.g.:
#   make update-agents AGENT_REGISTRY=registry.gitlab.example.com/group/project AGENT_TOOLS="claude opencode codex gemini"
PUBLIC_AGENT_REGISTRY ?= ghcr.io/wisdoverse/wisdoverse-forge
AGENT_REGISTRY ?= $(or $(shell grep -m1 '^AGENT_REGISTRY=' docker/.env 2>/dev/null | cut -d= -f2),$(PUBLIC_AGENT_REGISTRY))
PUBLIC_AGENT_TOOLS ?= opencode codex gemini
AGENT_TOOLS ?= $(PUBLIC_AGENT_TOOLS)

.PHONY: update-agent-base
update-agent-base: ## Pull agent base image from registry using GHCR_IMAGE_TAG
	@if [ -z "$(AGENT_REGISTRY)" ]; then \
		echo "Error: AGENT_REGISTRY not set."; \
		exit 1; \
	fi
	docker pull $(AGENT_REGISTRY)/agent-base:$(GHCR_IMAGE_TAG)
	docker tag $(AGENT_REGISTRY)/agent-base:$(GHCR_IMAGE_TAG) agentforge-agent-base:latest

.PHONY: update-agents
update-agents: update-agent-base ## Pull latest public agent images from registry
	@if [ -z "$(AGENT_REGISTRY)" ]; then \
		echo "Error: AGENT_REGISTRY not set."; \
		echo "Set it in docker/.env or pass via: make update-agents AGENT_REGISTRY=$(PUBLIC_AGENT_REGISTRY)"; \
		exit 1; \
	fi
	@echo "Pulling latest agent images from $(AGENT_REGISTRY) for: $(AGENT_TOOLS)"
	@UPDATED=0; \
	CLAUDE_UPDATED=0; \
	for tool in $(AGENT_TOOLS); do \
		echo "  agent-$$tool..."; \
		if ! docker pull $(AGENT_REGISTRY)/agent-$$tool:latest 2>/dev/null; then \
			echo "  SKIP: agent-$$tool not available in registry"; \
			continue; \
		fi; \
		docker tag $(AGENT_REGISTRY)/agent-$$tool:latest agentforge-agent:$$tool || \
			{ echo "  ERROR: Failed to tag agent-$$tool (check disk space / permissions)"; continue; }; \
		docker tag agentforge-agent:$$tool agentforge-agent-$$tool:latest 2>/dev/null || true; \
		if [ "$$tool" = "claude" ]; then CLAUDE_UPDATED=1; fi; \
		UPDATED=$$((UPDATED + 1)); \
	done; \
	if [ "$$UPDATED" -eq 0 ]; then \
		echo "ERROR: No agent images were updated. Check registry connectivity."; \
		exit 1; \
	fi; \
	if [ "$$CLAUDE_UPDATED" -eq 1 ]; then \
		docker tag agentforge-agent:claude agentforge-agent:latest || \
			echo "WARNING: Could not create agentforge-agent:latest alias (claude image missing)"; \
	else \
		echo "INFO: Public releases do not include agent-claude. Run 'make build-agent CLI_TOOL=claude' locally after accepting the vendor terms."; \
	fi; \
	echo "Done. $$UPDATED agent image(s) updated. New sessions will use updated images."
	@$(MAKE) sync-env

.PHONY: agent-versions
agent-versions: ## Show installed CLI tool versions in agent images
	@echo "Base:"
	@printf "  %-10s %s\n" "base" "$$(docker inspect agentforge-agent-base:latest --format='{{.Id}}' 2>/dev/null | cut -c8-19 || echo 'not built')"
	@echo "CLI images:"
	@for tool in $(ALL_AGENT_TOOLS); do \
		VER=$$(docker inspect agentforge-agent:$$tool --format='{{index .Config.Labels "org.wisdoverse.cli-version"}}' 2>/dev/null || echo "not installed"); \
		printf "  %-10s %s\n" "$$tool" "$$VER"; \
	done

.PHONY: sync-env
sync-env: ## Ensure docker/.env uses unversioned agent image tags (prune-safe)
	@echo "Syncing docker/.env with local agent images..."
	@for tool in $(ALL_AGENT_TOOLS); do \
		KEY="CONTAINER_IMAGE_$$(echo $$tool | tr '[:lower:]' '[:upper:]')"; \
		NEW_VAL="agentforge-agent:$$tool"; \
		VER=$$(docker inspect agentforge-agent:$$tool \
			--format='{{index .Config.Labels "org.wisdoverse.cli-version"}}' 2>/dev/null); \
		if [ -z "$$VER" ] || [ "$$VER" = "<no value>" ]; then \
			echo "  SKIP $$tool: image not found locally"; \
			continue; \
		fi; \
		OLD_VAL=$$(grep "^$$KEY=" docker/.env 2>/dev/null | cut -d= -f2); \
		if [ "$$OLD_VAL" = "$$NEW_VAL" ]; then \
			echo "  OK   $$tool: $$NEW_VAL (v$$VER)"; \
		else \
			sed -i "s|^$$KEY=.*|$$KEY=$$NEW_VAL|" docker/.env; \
			echo "  UPD  $$tool: $${OLD_VAL:-<unset>} → $$NEW_VAL (v$$VER)"; \
		fi; \
	done
	@echo "Done. Run 'make prod-ext' to apply changes."

# =============================================================================
# Utilities
# =============================================================================

.PHONY: detect-region
detect-region: ## Auto-detect network region and cache to .env.local
	@./scripts/detect-region.sh > .env.local
	@cat .env.local
	@echo ""
	@echo "Region config saved to .env.local. All 'make build-*' commands will use these mirrors."

check-mirrors: ## Check CN mirror source availability
	@./scripts/check-mirrors.sh

.PHONY: logs
logs: ## View logs for running services
	$(COMPOSE) logs -f

.PHONY: ps
ps: ## Show running containers
	$(COMPOSE) ps

.PHONY: shell
shell: ## Open shell in agentforge container
	docker exec -it agentforge sh

.PHONY: restart
restart: ## Restart agentforge container
	docker restart agentforge

.PHONY: health
health: ## Check container health
	@docker inspect agentforge --format='{{.State.Health.Status}}' 2>/dev/null || echo "Container not running"
	@curl -sf http://localhost:4003/health && echo " - API healthy" || echo "API not responding"

# =============================================================================
# Cleanup
# =============================================================================

.PHONY: down
down: ## Stop all containers (all profiles)
	$(COMPOSE) --profile '*' down

.PHONY: clean
clean: ## Stop all containers and remove volumes
	$(COMPOSE) --profile '*' down -v

.PHONY: clean-images
clean-images: ## Remove agentforge images
	docker rmi agentforge:latest agentforge:dev agentforge-platform:latest agentforge-agent:latest 2>/dev/null || true
	@for tool in $(ALL_AGENT_TOOLS); do \
		docker rmi agentforge-agent:$$tool agentforge-agent-$$tool:latest 2>/dev/null || true; \
	done

.PHONY: clean-all
clean-all: clean clean-images ## Remove everything (containers, volumes, images)
	docker system prune -f

# =============================================================================
# Platform Service (Go)
# =============================================================================

.PHONY: platform-proto-ts
platform-proto-ts: ## Generate TypeScript protobuf code from Rust-owned platform protos
	mkdir -p shared/generated/platform && \
	protoc \
		--plugin=./node_modules/.bin/protoc-gen-ts_proto \
		--ts_proto_out=shared/generated/platform \
		--ts_proto_opt=outputServices=grpc-js,esModuleInterop=true,env=node,useExactTypes=false \
		-I=rust/crates/platform/proto \
		rust/crates/platform/proto/*.proto

# =============================================================================
# Version
# =============================================================================

.PHONY: version
version: ## Show current version
	@node -p "require('./package.json').version"

.PHONY: version-patch
version-patch: ## Bump patch version (e.g. 0.1.15 → 0.1.16)
	npm version patch

.PHONY: version-minor
version-minor: ## Bump minor version (e.g. 0.1.15 → 0.2.0)
	npm version minor

.PHONY: version-major
version-major: ## Bump major version (e.g. 0.1.15 → 1.0.0)
	npm version major

# =============================================================================
# Frontend Deployment
# =============================================================================

.PHONY: deploy-frontend
deploy-frontend: ## Deploy frontend to webroot (requires WEBROOT_PATH)
	@if [ -z "$(WEBROOT_PATH)" ]; then \
		echo "Error: WEBROOT_PATH not set"; \
		echo "Usage: make deploy-frontend WEBROOT_PATH=/var/www/html"; \
		exit 1; \
	fi
	docker run --rm -v $(WEBROOT_PATH):/target agentforge:latest deploy-frontend /target

# =============================================================================
# Help
# =============================================================================

.PHONY: help
help: ## Show this help
	@echo "Wisdoverse Forge Docker Commands"
	@echo ""
	@echo "Usage: make <target>"
	@echo ""
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-18s\033[0m %s\n", $$1, $$2}'
	@echo ""
	@echo "Examples:"
	@echo "  make setup        One-time: create external networks"
	@echo "  make dev          Start development environment with Rust backend"
	@echo "  make quickstart-selfhost-pull DOMAIN=forge.example.com"
	@echo "                   Start and verify self-contained production from GHCR images"
	@echo "  make beginner-audit BEGINNER_AUDIT_FLAGS='--pull-images --local-smoke --live' DOMAIN=forge.example.com"
	@echo "                   Audit the beginner self-host path"
	@echo "  make quickstart-selfhost DOMAIN=forge.example.com"
	@echo "                   Same flow, building server/frontend images locally"
	@echo "  make prod-pull    Start production full stack from GHCR images"
	@echo "  make prod-ext     Start production with external services"

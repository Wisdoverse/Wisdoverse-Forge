# =============================================================================
# Wisdoverse Forge Frontend Artifact Image
# =============================================================================
#
# Purpose:
#   - Build the production frontend bundle
#   - Publish a lightweight image that can serve the static app when needed
#   - Provide the release artifact used by deploy.sh to extract /app/dist
#
# Backend runtime is Rust-owned and built from rust/Dockerfile.
#
# =============================================================================

ARG NODE_VERSION=24
ARG USER_UID=1000
ARG USER_GID=1000
ARG NPM_REGISTRY=

# -----------------------------------------------------------------------------
# Base Stage: Minimal runtime dependencies
# -----------------------------------------------------------------------------
FROM node:${NODE_VERSION}-slim AS base

LABEL org.opencontainers.image.title="Wisdoverse Forge Frontend" \
      org.opencontainers.image.description="Static frontend artifact image for Wisdoverse Forge" \
      org.opencontainers.image.vendor="Wisdoverse Forge" \
      org.opencontainers.image.source="https://github.com/Wisdoverse/Wisdoverse-Forge" \
      org.opencontainers.image.licenses="LicenseRef-Wisdoverse-Forge-BSL-1.1"

RUN apt-get update \
    && apt-get install -y --no-install-recommends tini curl git \
    && apt-get clean \
    && rm -rf /var/lib/apt/lists/* /tmp/* /var/tmp/*

WORKDIR /app

# -----------------------------------------------------------------------------
# Dependencies Stage: Install and cache npm packages
# -----------------------------------------------------------------------------
FROM base AS deps

ARG NPM_REGISTRY
RUN if [ -n "$NPM_REGISTRY" ]; then npm config set registry "$NPM_REGISTRY"; fi
COPY package*.json ./
RUN npm ci --legacy-peer-deps \
    && npm cache clean --force

# -----------------------------------------------------------------------------
# Development Target: Frontend-only dev server
# -----------------------------------------------------------------------------
FROM base AS development

ARG USER_UID
ARG USER_GID
RUN apt-get update \
    && apt-get install -y --no-install-recommends bash procps vim-tiny \
    && apt-get clean \
    && rm -rf /var/lib/apt/lists/*
ENV HOME=/tmp/agentforge
RUN mkdir -p "$HOME" \
    && chown -R ${USER_UID}:${USER_GID} /app "$HOME"
COPY --from=deps --chown=${USER_UID}:${USER_GID} /app/node_modules ./node_modules
COPY --chown=${USER_UID}:${USER_GID} . .
ENV NODE_ENV=development \
    AGENTFORGE_PORT=4003 \
    AGENTFORGE_CLIENT_PORT=4002 \
    NODE_OPTIONS="--enable-source-maps"
EXPOSE 4002
VOLUME ["/app/src", "/app/shared", "/app/public"]
USER ${USER_UID}:${USER_GID}
ENTRYPOINT ["/usr/bin/tini", "--"]
CMD ["npm", "run", "dev:client", "--", "--host", "0.0.0.0"]

# -----------------------------------------------------------------------------
# Builder Stage: Build frontend assets
# -----------------------------------------------------------------------------
FROM deps AS builder
COPY . .
ENV NODE_ENV=production \
    VITE_USE_PROXY=true
RUN npx vite build \
    && rm -rf node_modules/.cache

# -----------------------------------------------------------------------------
# Production Target: Static frontend artifact image
# -----------------------------------------------------------------------------
FROM base AS production

ARG USER_UID=1011
ARG USER_GID=1012
ARG APP_VERSION=unknown

LABEL org.opencontainers.image.version="${APP_VERSION}"

RUN groupadd -g ${USER_GID} agentforge \
    && useradd -u ${USER_UID} -g agentforge -m -s /sbin/nologin agentforge

COPY --chown=agentforge:agentforge package*.json ./
COPY --from=builder --chown=agentforge:agentforge /app/dist ./dist
COPY --chown=agentforge:agentforge docker/scripts/frontend-artifact-server.mjs /app/frontend-artifact-server.mjs
COPY --chown=agentforge:agentforge docker/scripts/entrypoint.sh /entrypoint.sh
RUN chmod +x /entrypoint.sh

ENV NODE_ENV=production \
    AGENTFORGE_PORT=4003 \
    VERSION=${APP_VERSION} \
    NO_UPDATE_NOTIFIER=1

EXPOSE 4003

HEALTHCHECK --interval=30s --timeout=10s --start-period=10s --retries=3 \
    CMD curl -sf http://localhost:${AGENTFORGE_PORT:-4003}/health/live || exit 1

USER agentforge
ENTRYPOINT ["/usr/bin/tini", "--", "/entrypoint.sh"]
CMD ["server"]

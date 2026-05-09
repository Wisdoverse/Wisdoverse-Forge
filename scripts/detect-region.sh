#!/usr/bin/env bash
# detect-region.sh — Detect network region and output mirror configuration.
#
# Priority:
#   1. REGION env var (manual override)
#   2. RUNNER_REGION env var (CI environment)
#   3. Network probe (curl registry.npmjs.org)
#
# Output: shell-sourceable variables to stdout
# Feedback: detection result to stderr
#
# Usage:
#   ./scripts/detect-region.sh > .env.local          # generate config
#   source <(./scripts/detect-region.sh)              # inline in CI
#   REGION=cn ./scripts/detect-region.sh > .env.local # force CN

set -euo pipefail

# Priority 1: explicit REGION override
if [ -n "${REGION:-}" ]; then
  DETECTED="$REGION"
  echo "Region: $DETECTED (from REGION env)" >&2

# Priority 2: CI runner region
elif [ -n "${RUNNER_REGION:-}" ]; then
  DETECTED="$RUNNER_REGION"
  echo "Region: $DETECTED (from RUNNER_REGION env)" >&2

# Priority 3: network probe
else
  PROBE_URL="https://registry.npmjs.org/-/ping"
  TIMEOUT_MS=1000

  START=$(date +%s%N 2>/dev/null || echo 0)
  if curl --connect-timeout 3 -sf "$PROBE_URL" >/dev/null 2>&1; then
    END=$(date +%s%N 2>/dev/null || echo 0)
    if [ "$START" != "0" ] && [ "$END" != "0" ]; then
      ELAPSED_MS=$(( (END - START) / 1000000 ))
    else
      ELAPSED_MS=0
    fi

    if [ "$ELAPSED_MS" -lt "$TIMEOUT_MS" ]; then
      DETECTED="global"
      echo "Region: global (probe ${ELAPSED_MS}ms < ${TIMEOUT_MS}ms threshold)" >&2
    else
      DETECTED="cn"
      echo "Region: cn (probe ${ELAPSED_MS}ms >= ${TIMEOUT_MS}ms threshold)" >&2
    fi
  else
    DETECTED="cn"
    echo "Region: cn (probe failed/timeout)" >&2
  fi
fi

# Output configuration
GENERATED=$(date '+%Y-%m-%d %H:%M:%S' 2>/dev/null || echo "unknown")

if [ "$DETECTED" = "cn" ]; then
  cat <<EOF
# Generated: $GENERATED, Region: cn
REGION=cn
NPM_REGISTRY=https://registry.npmmirror.com
DOCKER_MIRROR=https://mirrors.aliyun.com/docker-ce
GITHUB_PROXY=https://ghfast.top/
GHCR_MIRROR=ghcr.m.daocloud.io
EOF
else
  cat <<EOF
# Generated: $GENERATED, Region: global
REGION=global
NPM_REGISTRY=
DOCKER_MIRROR=
GITHUB_PROXY=
GHCR_MIRROR=
EOF
fi

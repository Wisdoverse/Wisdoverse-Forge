#!/usr/bin/env bash
# check-mirrors.sh — CN mirror source availability check
#
# Probes all CN mirror endpoints and reports health status.
# Usage: ./scripts/check-mirrors.sh
# CI:    MIRROR_CHECK_STRICT=1 ./scripts/check-mirrors.sh  (non-zero exit on failure)
#
# Exit codes:
#   0 = all mirrors healthy (or non-strict mode)
#   1 = one or more mirrors unreachable (strict mode only)

set -euo pipefail

TIMEOUT=5
FAILED=0
TOTAL=0

check() {
  local name="$1" url="$2"
  TOTAL=$((TOTAL + 1))
  if curl --connect-timeout "$TIMEOUT" -fsSL -o /dev/null "$url" 2>/dev/null; then
    printf "  ✓ %-20s %s\n" "$name" "$url"
  else
    printf "  ✗ %-20s %s (unreachable)\n" "$name" "$url" >&2
    FAILED=$((FAILED + 1))
  fi
}

echo "CN Mirror Health Check"
echo "━━━━━━━━━━━━━━━━━━━━━━"

check "npm (npmmirror)"    "https://registry.npmmirror.com/-/ping"
check "Go (goproxy.cn)"    "https://goproxy.cn/github.com/docker/compose/@v/list"
check "Alpine (aliyun)"    "https://mirrors.aliyun.com/alpine/latest-stable/main/"
check "Docker CE (aliyun)" "https://mirrors.aliyun.com/docker-ce/linux/static/stable/"
check "GitHub (ghfast)"    "https://ghfast.top"

echo "━━━━━━━━━━━━━━━━━━━━━━"
echo "Result: $((TOTAL - FAILED))/$TOTAL mirrors healthy"

if [ "$FAILED" -gt 0 ] && [ "${MIRROR_CHECK_STRICT:-0}" = "1" ]; then
  echo "STRICT mode: $FAILED mirror(s) unreachable, exiting with error" >&2
  exit 1
fi

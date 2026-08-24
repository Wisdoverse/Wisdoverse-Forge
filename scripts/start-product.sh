#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CLIENT_PORT="${AGENTFORGE_CLIENT_PORT:-4002}"
CLIENT_URL="http://localhost:${CLIENT_PORT}"

STACK_TOUCHED=0
VITE_PID=""

log() { printf '[product] %s\n' "$*"; }
die() { printf '[product] ERROR: %s\n' "$*" >&2; exit 1; }

open_browser() {
  local url="$1"
  if command -v xdg-open >/dev/null 2>&1; then
    xdg-open "$url" >/dev/null 2>&1 || true
  elif command -v open >/dev/null 2>&1; then
    open "$url" >/dev/null 2>&1 || true
  elif command -v cmd.exe >/dev/null 2>&1; then
    cmd.exe /c start "" "$url" >/dev/null 2>&1 || true
  fi
}

wait_for_url() {
  local url="$1" timeout="${2:-60}" tries=0
  while ! curl -sf -o /dev/null "$url"; do
    tries=$((tries + 1))
    if [ "$tries" -ge "$timeout" ]; then
      return 1
    fi
    sleep 1
  done
  return 0
}

cleanup() {
  local code=$?
  if [ -n "$VITE_PID" ] && kill -0 "$VITE_PID" 2>/dev/null; then
    kill "$VITE_PID" >/dev/null 2>&1 || true
    wait "$VITE_PID" 2>/dev/null || true
    VITE_PID=""
  fi
  if [ "$STACK_TOUCHED" = "1" ]; then
    STACK_TOUCHED=0
    log "Stopping the Forge services this command started..."
    (cd "$ROOT_DIR" && make dev-down) >/dev/null 2>&1 || true
  fi
  exit "$code"
}
trap cleanup EXIT INT TERM

cd "$ROOT_DIR"

# ---------------------------------------------------------------------------
# 1. Dependencies
# ---------------------------------------------------------------------------
command -v node >/dev/null 2>&1 || die "Node.js is required. Install Node.js 24+ and run this command again."
if [ ! -d node_modules ]; then
  log "Installing app dependencies (first run)..."
  npm install
fi

# ---------------------------------------------------------------------------
# 2. Backend stack
# ---------------------------------------------------------------------------
# bootstrap-local creates docker/.env (without overwriting) and checks tools.
# First runs sometimes pull the nats-box helper image and race once; retry.
log "Preparing the local environment..."
if ! make bootstrap-local; then
  log "Retrying local preparation once..."
  make bootstrap-local
fi

if bash scripts/check-local-runtime.sh >/dev/null 2>&1; then
  log "Forge services are already running."
else
  log "Starting Forge services (this can take a few minutes on first run)..."
  STACK_TOUCHED=1
  make dev-d
  if ! bash scripts/check-local-runtime.sh --wait --timeout 300; then
    die "Forge services did not become healthy in time. Run 'make dev-logs' for the log tail, or 'make dev-down' and try again."
  fi
fi

# ---------------------------------------------------------------------------
# 3. Browser app
# ---------------------------------------------------------------------------
if wait_for_url "$CLIENT_URL" 3; then
  log "Browser app is already running at $CLIENT_URL"
  open_browser "$CLIENT_URL"
  exit 0
fi

if [ ! -d node_modules/vite ]; then
  die "Vite is missing (npm install did not complete). Run 'npm install' and try again."
fi

log "Starting the browser app at $CLIENT_URL ..."
npm run dev &
VITE_PID=$!

# Wait for the app to answer; bail early if the dev server process dies.
tries=0
while ! curl -sf -o /dev/null "$CLIENT_URL"; do
  if ! kill -0 "$VITE_PID" 2>/dev/null; then
    die "Vite exited before the browser app was ready. Run 'npm install' and check whether port $CLIENT_PORT is already used."
  fi
  tries=$((tries + 1))
  if [ "$tries" -ge 90 ]; then
    die "The browser app did not come up on $CLIENT_URL within 90s. Check 'npm install' completed and port $CLIENT_PORT is free."
  fi
  sleep 1
done

open_browser "$CLIENT_URL"

printf '\n'
log "Wisdoverse Forge is ready."
log "  Open:  $CLIENT_URL"
log "  First time? Register an account, then follow the Start checklist."
log "  Stop:   Press Ctrl+C here (also stops the services this command started),"
log "          or run 'make dev-down' later."
printf '\n'

# Keep the browser app attached to this terminal. Ctrl+C stops it and (through
# the EXIT trap) the services this command started.
wait "$VITE_PID"
VITE_PID=""
exit 0

#!/bin/sh
set -eu

log() {
  printf '%s\n' "$*"
}

run_healthcheck() {
  curl -sf "http://localhost:${AGENTFORGE_PORT:-4003}/health/live" >/dev/null
}

start_server() {
  if [ ! -f /app/dist/index.html ]; then
    log "entrypoint: frontend artifact not found at /app/dist/index.html"
    exit 1
  fi

  exec node /app/frontend-artifact-server.mjs
}

cmd="${1:-server}"
case "$cmd" in
  server|start)
    start_server
    ;;
  health|healthcheck)
    run_healthcheck
    ;;
  sh|bash|shell)
    exec /bin/sh
    ;;
  --)
    shift
    exec "$@"
    ;;
  *)
    exec "$@"
    ;;
esac

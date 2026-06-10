#!/bin/sh
# Wisdoverse Forge — one-command local agent join.
#
# Rendered and served by the control plane at
#   GET /api/v1/agents/local-join/script
# Usage (shown in the Create Agent dialog):
#   curl -fsSL <server>/api/v1/agents/local-join/script | sh -s -- --code afj_...
set -eu

SERVER_URL="__AGENTFORGE_SERVER_URL__"
BINARY_BASE_URL="__AGENTFORGE_BINARY_BASE_URL__"
JOIN_CODE="${AGENTFORGE_JOIN_CODE:-}"
WORK_DIR=""

while [ $# -gt 0 ]; do
    case "$1" in
        --code) JOIN_CODE="${2:-}"; shift 2 ;;
        --cwd) WORK_DIR="${2:-}"; shift 2 ;;
        *) echo "error: unknown option: $1" >&2; exit 2 ;;
    esac
done

if [ -z "$JOIN_CODE" ]; then
    echo "error: missing pairing code. Re-copy the full join command from the Create Agent dialog." >&2
    exit 2
fi

# --- 1. Locate or download the sidecar -------------------------------------
BIN_DIR="${HOME}/.agentforge/bin"
SIDECAR="$(command -v agentforge-sidecar 2>/dev/null || true)"
if [ -z "$SIDECAR" ] && [ -x "$BIN_DIR/agentforge-sidecar" ]; then
    SIDECAR="$BIN_DIR/agentforge-sidecar"
fi
if [ -z "$SIDECAR" ]; then
    OS="$(uname -s)"
    ARCH="$(uname -m)"
    case "$OS" in
        Linux) OS=linux ;;
        Darwin) OS=macos ;;
        *) echo "error: unsupported OS '$OS'. Use the manual setup shown in the Create Agent dialog." >&2; exit 1 ;;
    esac
    case "$ARCH" in
        x86_64|amd64) ARCH=amd64 ;;
        arm64|aarch64) ARCH=arm64 ;;
        *) echo "error: unsupported architecture '$ARCH'." >&2; exit 1 ;;
    esac
    ASSET="agentforge-sidecar-$OS-$ARCH"
    echo "Downloading $ASSET ..."
    mkdir -p "$BIN_DIR"
    curl -fL --progress-bar "$BINARY_BASE_URL/$ASSET" -o "$BIN_DIR/agentforge-sidecar"
    chmod +x "$BIN_DIR/agentforge-sidecar"
    SIDECAR="$BIN_DIR/agentforge-sidecar"
    echo "Downloaded to $SIDECAR."
    echo "Tip: you can verify release binaries with 'agentforge verify' (see the Host CLI runbook)."
fi

# --- 2. Exchange the pairing code for this agent's environment -------------
ENV_DIR="${HOME}/.agentforge/agents"
mkdir -p "$ENV_DIR"
ENV_FILE="$ENV_DIR/.join-$$.env"
HTTP_CODE="$(curl -sS -o "$ENV_FILE" -w '%{http_code}' \
    -X POST "$SERVER_URL/api/v1/agents/local-join/claim" \
    -H 'content-type: application/json' \
    --data "{\"code\":\"$JOIN_CODE\",\"format\":\"exports\"}")" || {
    rm -f "$ENV_FILE"
    echo "error: could not reach $SERVER_URL. Check your network and try again." >&2
    exit 1
}
if [ "$HTTP_CODE" != "200" ]; then
    rm -f "$ENV_FILE"
    echo "error: pairing code rejected (HTTP $HTTP_CODE)." >&2
    echo "Codes expire after 15 minutes. Create the agent again in the dialog to get a fresh command." >&2
    exit 1
fi
chmod 600 "$ENV_FILE"

# --- 3. Load the environment and keep it for later restarts ----------------
set -a
. "$ENV_FILE"
set +a
AGENT_ID_SAFE="$(printf '%s' "${AGENT_ID:-agent}" | tr -cd 'a-zA-Z0-9-')"
FINAL_ENV="$ENV_DIR/${AGENT_ID_SAFE:-agent}.env"
mv "$ENV_FILE" "$FINAL_ENV"

# --- 4. Friendly preflight ---------------------------------------------------
TOOL="${CLI_TOOL:-}"
if [ -n "$TOOL" ] && ! command -v "$TOOL" >/dev/null 2>&1; then
    echo "warning: '$TOOL' is not installed on this machine. Install it before sending tasks to this agent." >&2
fi
if [ -n "$WORK_DIR" ]; then
    cd "$WORK_DIR"
fi

echo ""
echo "Agent connected. Leave this window open while the agent is in use; press Ctrl+C to disconnect."
echo "Environment saved to $FINAL_ENV — reconnect later with:"
echo "  sh -c 'set -a; . $FINAL_ENV; set +a; exec agentforge-sidecar'"
echo ""
exec "$SIDECAR"

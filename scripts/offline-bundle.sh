#!/usr/bin/env bash
# Package a self-hosted offline bundle: server + agent-base images as one
# verifiable tar for air-gapped hosts.
#
# Prereqs (on an internet-connected host):
#   make build-agent-base   # builds the agent base image
#   make prod-ext           # builds the server image (or docker compose build)
#
# Usage:
#   scripts/offline-bundle.sh            # writes dist/offline-bundle-<version>.tar.gz
#   scripts/offline-bundle.sh --dry-run  # print the commands without running them
#   VERSION=0.1.15 scripts/offline-bundle.sh
set -euo pipefail

DRY_RUN=0
FULL_STACK=0
for arg in "$@"; do
  case "$arg" in
    --dry-run) DRY_RUN=1 ;;
    --full-stack) FULL_STACK=1 ;;
    -h|--help) grep '^#' "$0" | sed 's/^# //' | head -20; exit 0 ;;
    *) echo "Unknown argument: $arg (use --dry-run)" >&2; exit 2 ;;
  esac
done

VERSION="${VERSION:-$(node -p 'require("./package.json").version' 2>/dev/null || echo latest)}"
SERVER_IMAGE="${AGENTFORGE_SERVER_IMAGE:-agentforge-server:${VERSION}}"
AGENT_IMAGE="${AGENT_BASE_IMAGE:-agentforge/agent-base:${VERSION}}"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT_DIR="${OUT_DIR:-$ROOT/dist}"
BUNDLE="$OUT_DIR/offline-bundle-${VERSION}.tar.gz"
TUF_STATE_DIR="${TUF_STATE_DIR:-$OUT_DIR/offline-bundle-tuf}"

# Optional: include the whole compose stack (db, redis, nats, temporal) so a
# fully air-gapped host needs no registry access at all.
IMAGES="$SERVER_IMAGE $AGENT_IMAGE"
if [ "$FULL_STACK" = 1 ]; then
  # Pinned platform services from docker/compose.yml; override with
  # STACK_IMAGES="..." when your stack differs.
  STACK_IMAGES="${STACK_IMAGES:-agentforge-frontend:${VERSION} postgres:18-alpine redis:8-alpine nats:2.12.7-alpine temporalio/auto-setup:1.26 minio/minio:latest}"
  IMAGES="$IMAGES $STACK_IMAGES"
fi

say() { [ "$DRY_RUN" = 1 ] && echo "[dry-run] $*" || echo "$*"; }
run() { if [ "$DRY_RUN" = 1 ]; then say "$*"; else "$@"; fi }

if [ "$DRY_RUN" = 0 ]; then
  if [ -z "${BUNDLE_SIGNING_KEY:-}" ] || [ ! -f "$BUNDLE_SIGNING_KEY" ]; then
    echo "Set BUNDLE_SIGNING_KEY to an Ed25519 private key before building." >&2
    exit 1
  fi
  SIGNING_KEY="$(cd "$(dirname "$BUNDLE_SIGNING_KEY")" && pwd)/$(basename "$BUNDLE_SIGNING_KEY")"
  if ! command -v agentforge >/dev/null 2>&1; then
    echo "agentforge CLI is required to create trusted offline metadata." >&2
    echo "Build it with: cd rust && cargo build -p agentforge-cli-bin" >&2
    exit 1
  fi
  mkdir -p "$OUT_DIR"
  WORK_DIR="$(mktemp -d "$OUT_DIR/.offline-bundle.XXXXXX")"
  trap 'rm -rf "$WORK_DIR"' EXIT
  if [ -d "$TUF_STATE_DIR/metadata" ]; then
    cp -R "$TUF_STATE_DIR/metadata" "$WORK_DIR/"
  fi
fi

for image in $IMAGES; do
  if [ "$DRY_RUN" = 0 ] && ! docker image inspect "$image" >/dev/null 2>&1; then
    echo "Image not found locally: $image" >&2
    echo "Build/bring it first: make build-agent-base; make prod-ext (or docker compose build; docker pull)" >&2
    exit 1
  fi
  say "Using image: $image"
done

if [ "$DRY_RUN" = 0 ]; then
  printf '%s\n' $IMAGES > "$WORK_DIR/images.txt"
  cat > "$WORK_DIR/README.txt" <<EOF
Wisdoverse Forge offline bundle $VERSION

Contents:
  images.tar  - docker save output (server + agent base)
  images.txt  - the image tags inside the bundle
  SHA256SUMS  - integrity checksums for this directory

On the air-gapped host:
  scripts/load-offline-bundle.sh $BUNDLE
  then follow docs/guides/offline-install.md.
EOF
fi

say "Saving images..."
run docker save -o "${WORK_DIR:-$OUT_DIR/offline-bundle}/images.tar" $IMAGES

if [ "$DRY_RUN" = 0 ]; then
  ( cd "$WORK_DIR" && sha256sum images.tar images.txt README.txt > SHA256SUMS )
  ( cd "$WORK_DIR" && openssl pkeyutl -sign -rawin -in SHA256SUMS -inkey "$SIGNING_KEY" -out SHA256SUMS.sig )
  echo "Bundle checksums signed (SHA256SUMS.sig)."
  if [ -f "$WORK_DIR/metadata/root.json" ]; then
    ( cd "$WORK_DIR" && agentforge tuf sign --dir . --key "$SIGNING_KEY" )
  else
    ( cd "$WORK_DIR" && agentforge tuf init --dir . --key "$SIGNING_KEY" )
  fi
  mkdir -p "$TUF_STATE_DIR"
  cp -R "$WORK_DIR/metadata" "$TUF_STATE_DIR/"
  tar -C "$WORK_DIR" -czf "$BUNDLE" .
  echo "Created: $BUNDLE"
  ls -lh "$BUNDLE"
else
  say "docker save -o ... images.tar $IMAGES"
  say "sha256sum images.tar images.txt README.txt > SHA256SUMS"
  say "tar -C $OUT_DIR/offline-bundle -czf $BUNDLE ."
fi

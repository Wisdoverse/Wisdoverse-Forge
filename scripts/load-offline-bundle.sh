#!/usr/bin/env bash
# Load and verify an offline bundle created by scripts/offline-bundle.sh.
#
# Usage: scripts/load-offline-bundle.sh /path/to/offline-bundle-<version>.tar.gz [signer.pub]
set -euo pipefail

BUNDLE="${1:-}"
if [ -z "$BUNDLE" ]; then
  echo "Usage: scripts/load-offline-bundle.sh <bundle.tar.gz>" >&2
  echo "Example: scripts/load-offline-bundle.sh dist/offline-bundle-0.1.15.tar.gz" >&2
  exit 2
fi
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
WORK="$ROOT/dist/offline-bundle-loaded"

rm -rf "$WORK"
mkdir -p "$WORK"
tar -xzf "$BUNDLE" -C "$WORK"
SIG_PUBKEY="${2:-${BUNDLE_PUBLIC_KEY:-}}"
if [ -f "$WORK/SHA256SUMS.sig" ]; then
  if [ -z "$SIG_PUBKEY" ]; then
    echo "Bundle is signed but no public key supplied." >&2
    echo "Pass it as the 2nd argument or set BUNDLE_PUBLIC_KEY." >&2
    exit 3
  fi
  ( cd "$WORK" && openssl pkeyutl -verify -rawin -in SHA256SUMS -inkey "$SIG_PUBKEY" -pubin -sigfile SHA256SUMS.sig )
  echo "Bundle signature verified."
elif [ -n "$SIG_PUBKEY" ]; then
  echo "Public key supplied but the bundle has no SHA256SUMS.sig." >&2
  exit 3
fi
# TUF-style trusted metadata (requires the agentforge CLI + a pinned root).
TUF_PIN="${TUF_PIN:-/etc/agentforge/tuf/root.json}"
if [ -f "$WORK/metadata/root.json" ]; then
  if command -v agentforge >/dev/null 2>&1; then
    if [ -f "$TUF_PIN" ]; then
      ( cd "$WORK" && agentforge tuf verify --dir . --pin "$TUF_PIN" )
    else
      echo "Bundle has TUF metadata but no pinned root at $TUF_PIN." >&2
      echo "On the FIRST host run, copy metadata/root.json from the bundle to $TUF_PIN" >&2
      echo "once (or pass TUF_PIN=<path>). Re-verify afterwards or exit." >&2
      exit 3
    fi
  else
    echo "agentforge CLI not found; skipping TUF verify (checksums still checked below)." >&2
  fi
fi
( cd "$WORK" && sha256sum -c SHA256SUMS )
docker image load -i "$WORK/images.tar"
echo "Loaded images from $BUNDLE. Verify with: docker images | grep agentforge"

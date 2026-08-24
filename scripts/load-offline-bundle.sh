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
( cd "$WORK" && sha256sum -c SHA256SUMS )
docker image load -i "$WORK/images.tar"
echo "Loaded images from $BUNDLE. Verify with: docker images | grep agentforge"

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
WORK="$(mktemp -d "${TMPDIR:-/tmp}/agentforge-offline-bundle.XXXXXX")"
trap 'rm -rf "$WORK"' EXIT
if tar -tzf "$BUNDLE" | grep -Eq '(^/|(^|/)\.\.(/|$))'; then
  echo "Bundle contains an unsafe archive path." >&2
  exit 3
fi
if tar -tvzf "$BUNDLE" | awk 'substr($1, 1, 1) != "-" && substr($1, 1, 1) != "d" { found=1 } END { exit !found }'; then
  echo "Bundle contains links or special files, which are not allowed." >&2
  exit 3
fi
tar --no-same-owner --no-same-permissions -xzf "$BUNDLE" -C "$WORK"
SIG_PUBKEY="${2:-${BUNDLE_PUBLIC_KEY:-}}"
TUF_PIN="${TUF_PIN:-/etc/agentforge/tuf/root.json}"
if [ -f "$WORK/metadata/root.json" ]; then
  if ! command -v agentforge >/dev/null 2>&1; then
    echo "Bundle uses TUF metadata but the agentforge CLI is not installed." >&2
    exit 3
  fi
  if [ ! -f "$TUF_PIN" ]; then
    echo "Bundle has TUF metadata but no pinned root at $TUF_PIN." >&2
    echo "Provision the trusted root out of band before loading the bundle." >&2
    exit 3
  fi
  TUF_PIN="$(cd "$(dirname "$TUF_PIN")" && pwd)/$(basename "$TUF_PIN")"
  ( cd "$WORK" && agentforge tuf verify --dir . --pin "$TUF_PIN" )
else
  if [ -f "$TUF_PIN" ]; then
    echo "Bundle has no TUF metadata; refusing a trust downgrade while $TUF_PIN exists." >&2
    exit 3
  fi
  if [ ! -f "$WORK/SHA256SUMS.sig" ] || [ -z "$SIG_PUBKEY" ] || [ ! -f "$SIG_PUBKEY" ]; then
    echo "Legacy bundle requires SHA256SUMS.sig and its trusted public key." >&2
    echo "Pass the public key as the 2nd argument or set BUNDLE_PUBLIC_KEY." >&2
    exit 3
  fi
  SIG_PUBKEY="$(cd "$(dirname "$SIG_PUBKEY")" && pwd)/$(basename "$SIG_PUBKEY")"
  ( cd "$WORK" && openssl pkeyutl -verify -rawin -in SHA256SUMS -inkey "$SIG_PUBKEY" -pubin -sigfile SHA256SUMS.sig )
  ( cd "$WORK" && sha256sum -c SHA256SUMS )
  echo "Legacy bundle signature and checksums verified."
fi
docker image load -i "$WORK/images.tar"
echo "Loaded images from $BUNDLE. Verify with: docker images | grep agentforge"

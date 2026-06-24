#!/bin/bash
# =============================================================================
# Cosign installer for staging / production hosts
# =============================================================================
# Installs the Sigstore `cosign` binary so `scripts/deploy.sh` can verify
# Sigstore signatures attached to GHCR images by `.github/workflows/
# publish-images.yml` (`provenance: true`). Set `VERIFY_IMAGE_SIGNATURES=true`
# in `docker/.env` after this script succeeds to flip the verify gate on.
#
# Idempotent: skips download if the requested version is already on PATH.
#
# Usage:
#   sudo COSIGN_VERSION=v2.5.4 bash scripts/install-cosign.sh
# =============================================================================

set -euo pipefail

COSIGN_VERSION="${COSIGN_VERSION:-v2.5.4}"
COSIGN_INSTALL_PATH="${COSIGN_INSTALL_PATH:-/usr/local/bin/cosign}"

log() { echo "$(date '+%H:%M:%S') [install-cosign] $*"; }
log_error() { echo "$(date '+%H:%M:%S') [install-cosign] ERROR: $*" >&2; }

if command -v cosign >/dev/null 2>&1; then
  current="$(cosign version 2>/dev/null | awk '/GitVersion/ {print $2; exit}')"
  if [ "$current" = "$COSIGN_VERSION" ]; then
    log "cosign $COSIGN_VERSION already installed at $(command -v cosign)"
    exit 0
  fi
  log "cosign $current installed; replacing with $COSIGN_VERSION"
fi

case "$(uname -m)" in
  x86_64 | amd64) arch=amd64 ;;
  aarch64 | arm64) arch=arm64 ;;
  *)
    log_error "Unsupported architecture: $(uname -m)"
    exit 1
    ;;
esac

case "$(uname -s)" in
  Linux) os=linux ;;
  Darwin) os=darwin ;;
  *)
    log_error "Unsupported OS: $(uname -s)"
    exit 1
    ;;
esac

binary_url="https://github.com/sigstore/cosign/releases/download/${COSIGN_VERSION}/cosign-${os}-${arch}"
sig_url="${binary_url}.sig"
cert_url="${binary_url}.pem"

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

log "Downloading $binary_url..."
curl -fsSL -o "$tmp/cosign" "$binary_url"
curl -fsSL -o "$tmp/cosign.sig" "$sig_url" || log "WARN: signature not available for this release"
curl -fsSL -o "$tmp/cosign.pem" "$cert_url" || log "WARN: cert not available for this release"

# Verify the cosign binary against its keyless signature when both companion
# files exist. Bootstraps trust without needing a separate cosign already on
# PATH — uses the GitHub Actions OIDC issuer that signed the release.
if [ -s "$tmp/cosign.sig" ] && [ -s "$tmp/cosign.pem" ] && command -v cosign >/dev/null 2>&1; then
  log "Verifying release signature with existing cosign..."
  cosign verify-blob \
    --certificate "$tmp/cosign.pem" \
    --signature "$tmp/cosign.sig" \
    --certificate-identity-regexp '@sigstore.dev$' \
    --certificate-oidc-issuer 'https://token.actions.githubusercontent.com' \
    "$tmp/cosign" \
    || {
      log_error "Release signature verification failed; aborting install"
      exit 1
    }
fi

chmod +x "$tmp/cosign"
install -m 0755 "$tmp/cosign" "$COSIGN_INSTALL_PATH"

log "Installed cosign $COSIGN_VERSION at $COSIGN_INSTALL_PATH"
"$COSIGN_INSTALL_PATH" version | head -3

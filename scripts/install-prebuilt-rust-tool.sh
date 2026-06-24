#!/usr/bin/env bash
# Install a pinned Rust CI helper binary from its upstream GitHub release
# as a pre-built tarball. Replaces `cargo install <tool> --locked`, which
# rebuilt the tool from source on every cache-miss (5-10 min per tool).
#
# Usage: scripts/install-prebuilt-rust-tool.sh <tool>
# <tool> ∈ { sccache | cargo-nextest | cargo-audit | cargo-auditable }
#
# Idempotent: skips if $CARGO_HOME/bin/<tool> already exists (cache hit).
#
# Versions AND sha256 values are pinned in this script — drift requires a
# deliberate edit and review, not an ambient upstream pickup. Supply-chain
# guardrail matches the cargo-audit / rust-audit-bin posture elsewhere in
# this pipeline.
#
# Fallback: if the GitHub release download fails (network, rate limit,
# upstream outage, etc.), the script falls back to
# `cargo install <tool> --locked --version <version>` from crates.io. That
# rebuild costs 5-10 min per tool per cache miss (the exact pain the
# prebuilt path was meant to avoid), but keeps CI unblocked during
# upstream GitHub incidents — CI dead for 30min is worse than CI slow for
# 10min. Issue #51 filed the prior outage that motivated the fallback.

set -euo pipefail

tool="${1:-}"
: "${CARGO_HOME:?CARGO_HOME must be set}"

# Validate the tool arg before deriving `$target` — an empty `$tool`
# would otherwise produce `$CARGO_HOME/bin/`, a directory that passes
# `-x` and silently short-circuits the install to a no-op "already
# present" success.
case "$tool" in
  sccache | cargo-nextest | cargo-audit | cargo-auditable)
    ;;
  "")
    echo "[install-prebuilt-rust-tool] usage: $0 <tool>" >&2
    exit 2
    ;;
  *)
    echo "[install-prebuilt-rust-tool] unknown tool: $tool" >&2
    exit 2
    ;;
esac

target="$CARGO_HOME/bin/$tool"

if [ -x "$target" ]; then
  echo "[install-prebuilt-rust-tool] $tool already present at $target — skipping"
  exit 0
fi

case "$tool" in
  sccache)
    version=0.8.2
    sha256=ecda4ddc89a49f1ec6f35bdce5ecbf6f205b399a680d11119d4ce9f6d962104e
    url="https://github.com/mozilla/sccache/releases/download/v${version}/sccache-v${version}-x86_64-unknown-linux-musl.tar.gz"
    archive_fmt=gz
    inner_path="sccache-v${version}-x86_64-unknown-linux-musl/sccache"
    ;;
  cargo-nextest)
    version=0.9.80
    sha256=a7010deb839d2967c0b79504f88d1a78f01cbb16b1cd9eae749b3973719ab5af
    url="https://github.com/nextest-rs/nextest/releases/download/cargo-nextest-${version}/cargo-nextest-${version}-x86_64-unknown-linux-musl.tar.gz"
    archive_fmt=gz
    inner_path="cargo-nextest"
    ;;
  cargo-audit)
    version=0.22.1
    sha256=c32506f338bdcdaef5a17fb9f33abb6ecf9561324cfd34237fd335f9283a1eab
    url="https://github.com/rustsec/rustsec/releases/download/cargo-audit%2Fv${version}/cargo-audit-x86_64-unknown-linux-musl-v${version}.tgz"
    archive_fmt=gz
    inner_path="cargo-audit-x86_64-unknown-linux-musl-v${version}/cargo-audit"
    ;;
  cargo-auditable)
    version=0.7.4
    sha256=4a4f0c124543c065f03d89aee26550305143c6e4af3e46270dbabefeb79895d2
    url="https://github.com/rust-secure-code/cargo-auditable/releases/download/v${version}/cargo-auditable-x86_64-unknown-linux-musl.tar.xz"
    archive_fmt=xz
    inner_path="cargo-auditable-x86_64-unknown-linux-musl/cargo-auditable"
    ;;
esac

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

# Optional reverse-proxy prefix. CI jobs may set CN_GITHUB_PROXY so that
# `github.com` fetches go through a reachable mirror. When unset, the upstream
# URL is used verbatim.
#
# The sha256 check below still runs against the original pinned digest,
# so a compromised proxy cannot substitute a different binary — the
# mirror only changes the *route*, not the *trust anchor*.
fetch_url="${GH_MIRROR_PREFIX:-}${url}"

# Try the pinned prebuilt tarball first. Wall-clock bounded per attempt
# (`--connect-timeout 20`, `--max-time 180`) so an unreachable upstream
# — proxy or origin — returns control to the shell in ~3 min worst case,
# instead of silently hanging until the 30-min GitLab job timeout fires.
# Issue #51 recorded the prior silent-hang pattern.
fetch_prebuilt() {
  echo "[install-prebuilt-rust-tool] fetching $tool v$version from ${fetch_url}"
  curl --fail --silent --show-error --location \
       --connect-timeout 20 \
       --max-time 180 \
       --retry 5 \
       --retry-delay 2 \
       --retry-connrefused \
       --retry-all-errors \
       --output "$tmp/tool.archive" "$fetch_url"
}

install_from_tarball() {
  echo "${sha256}  $tmp/tool.archive" | sha256sum --check --strict -

  case "$archive_fmt" in
    gz) tar -xzf "$tmp/tool.archive" -C "$tmp" ;;
    xz) tar -xJf "$tmp/tool.archive" -C "$tmp" ;;
  esac

  mkdir -p "$(dirname "$target")"
  install -m 0755 "$tmp/$inner_path" "$target"
  echo "[install-prebuilt-rust-tool] $tool v$version installed from tarball at $target"
}

# Fallback path: build from crates.io. Slow (5-10 min) but always reachable
# via the CN mirror when GitHub is unreliable — cargo honors the runner's
# `CARGO_HTTP_*` and any `[source.crates-io]` replacement the gitlab-ci
# template wires up. Version is pinned so we rebuild the same bits the
# prebuilt would have carried; only the provenance changes (crates.io vs
# github release binary).
install_from_crates_io() {
  echo "[install-prebuilt-rust-tool] prebuilt fetch failed — building $tool v$version from crates.io (fallback)"
  echo "[install-prebuilt-rust-tool] see issue #51 for why this branch exists; expect 5-10min"
  cargo install "$tool" --locked --version "$version"
  echo "[install-prebuilt-rust-tool] $tool v$version installed from crates.io at $target"
}

if fetch_prebuilt; then
  install_from_tarball
else
  install_from_crates_io
fi

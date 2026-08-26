# Offline install (air-gapped)

Wisdoverse Forge can be moved to a host with no internet access by building
the images on a connected host, packaging them into one verifiable bundle,
and loading them on the target.

## 1. Build on an internet-connected host

```bash
make setup
make build-agent-base      # agent runtime base image
make prod-ext              # builds agentforge-server (and orchestrator/frontend)
docker pull postgres:18-alpine redis:8-alpine nats:2.12.7-alpine temporalio/auto-setup:1.26 minio/minio:latest
```

> `make prod-ext` builds `agentforge-server:latest`. To match a version tag,
> set `VERSION` (e.g. `VERSION=0.1.15 make prod-ext`).

### Signing and trusted metadata

Generate an Ed25519 keypair on the connected host (keep the private key
offline):

```bash
openssl genpkey -algorithm ed25519 -out bundle-signing.key
openssl pkey -in bundle-signing.key -pubout -out bundle-signing.pub
```

Build and install the `agentforge` CLI on the connected host, then run the
builder with `BUNDLE_SIGNING_KEY=bundle-signing.key`. Both are required; the
builder refuses to create an unsigned bundle. The private key never travels.

The builder emits a TUF-style metadata chain under
`metadata/` — `root.json`, `targets.json`, `snapshot.json`, `timestamp.json` —
signed by the same Ed25519 key. The chain pins file hashes + sizes (targets),
hashes targets (snapshot), and hashes snapshot (timestamp); every verify walks
root → signature → timestamp → snapshot → targets → on-disk file bytes.
Root and child versions persist in `dist/offline-bundle-tuf` between builds;
set `TUF_STATE_DIR` when that state belongs elsewhere.

### Root pinning (one time per host)

On the connected host, copy the root metadata once **per host** that will load
bundles (this is the trust anchor; the root file is public — keep a durable
copy of it somewhere safe):

```bash
tar -xzf dist/offline-bundle-0.1.15.tar.gz -C /tmp/bundle && \
  install -d /etc/agentforge/tuf && \
  cp /tmp/bundle/metadata/root.json /etc/agentforge/tuf/root.json
```

Or pass `TUF_PIN=<path>` to the loader on first run; after that the pin is
checked on every load. **Do not** copy a new root from every bundle — a newer
root is accepted only when it is exactly one version newer and satisfies both
the pinned and candidate root thresholds. A downgrade is always rejected.
The host persists only root state: replay within the same root remains bounded
by signed child expiry, so rotate the root when an image must be revoked.

### Verifying on the air-gapped host

`scripts/load-offline-bundle.sh dist/offline-bundle-*.tar.gz` verifies the TUF
chain automatically when `metadata/root.json` is present, the `agentforge`
CLI is installed, and a pin exists. Missing prerequisites abort. Manually:

```bash
agentforge tuf verify --dir /tmp/bundle --pin /etc/agentforge/tuf/root.json
# TUF chain verified: root v1 (pinned), N targets, signatures + hash chain OK.
```

### Key rotation

Sign a NEW root with the old and new private keys (the old key proves the
rotation; the new key becomes primary with a grace period where both are
trusted), then start issuing bundles with the new key:

```bash
openssl genpkey -algorithm ed25519 -out bundle-signing-new.key
agentforge tuf rotate --dir dist/offline-bundle-tuf --new-key bundle-signing-new.key --old-key bundle-signing.key
BUNDLE_SIGNING_KEY=bundle-signing-new.key scripts/offline-bundle.sh --full-stack
```

Hosts with the old pin accept the next root only after both root thresholds and
the complete payload chain verify. The verifier then atomically advances the
local pin; operators do not copy roots from later bundles.

## 3. Package the bundle

```bash
BUNDLE_SIGNING_KEY=bundle-signing.key scripts/offline-bundle.sh --full-stack
scripts/offline-bundle.sh --dry-run           # preview the commands without running them
```

The bundle contains `images.tar` (docker save output), `images.txt` (the tag
list), `README.txt`, `SHA256SUMS` and, when signed, `SHA256SUMS.sig`. `--full-stack` also includes the
platform services (PostgreSQL, Redis, NATS, Temporal, MinIO); omit it when
only the Forge images are needed and the target already runs those services.

## 4. Load on the air-gapped host

Transfer the bundle (USB, jump host, …) and run:

```bash
scripts/load-offline-bundle.sh dist/offline-bundle-0.1.15.tar.gz bundle-signing.pub
```

The loader extracts into a temporary directory and requires one trusted path:
TUF metadata verified against the host pin, or a legacy `SHA256SUMS.sig`
verified with the supplied public key when no host pin exists. It rejects
unsigned bundles and TUF downgrades before `docker image load`.

## 5. Start

Copy your `docker/.env` (DATABASE_URL/JWT_SECRET/NATS passwords) onto the host
and start the stack as usual:

```bash
make prod-ext
```

## Troubleshooting

- **`Image not found locally`: build it first.** Images must already exist
  locally — the script never pulls from a registry.
- **`sha256sum: FAILED`**: the bundle is corrupted; re-transfer it.
- **Container CLI tools missing inside agents**: the agent-base image contains
  the platform CLIs; rebuild it on the connected host when you update the
  sidecar (`make build-agent-base`), then re-bundle.

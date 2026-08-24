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

### Signing (TUF-style starter)

Generate an Ed25519 keypair on the connected host (keep the private key
offline):

```bash
openssl genpkey -algorithm ed25519 -out bundle-signing.key
openssl pkey -in bundle-signing.key -pubout -out bundle-signing.pub
```

Then run the builder with `BUNDLE_SIGNING_KEY=bundle-signing.key`. The bundle
carries `SHA256SUMS.sig` (raw-in signature over the checksums file); the
private key never travels. Unsigned bundles warn loudly instead of failing.

## 3. Package the bundle

```bash
scripts/offline-bundle.sh --full-stack        # writes dist/offline-bundle-<version>.tar.gz
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

The loader extracts into `dist/offline-bundle-loaded`, verifies the signature
when `SHA256SUMS.sig` and the public key are present (abort on failure),
then verifies every file against `SHA256SUMS`, and finally
`docker image load -i images.tar`. Any mismatch aborts before anything is
loaded.

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

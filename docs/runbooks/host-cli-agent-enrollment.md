# Host CLI Agent Enrollment

Use this when an operator wants a CLI agent running on their own machine to join
the remote Wisdoverse Forge control plane.

This runbook is written for operators who may not be professional engineers.
Follow the steps in order. Advanced runtime details are intentionally kept after
the basic path.

## Requirements

- The remote API must be reachable by the operator.
- `NATS_AGENT_URL` must point to a NATS address reachable from the operator
  machine. The API strips user-info and injects per-agent credentials.
- `agentforge-sidecar` and the selected CLI (`claude`, `codex`, `gemini`, or
  `opencode`) must be installed on the operator machine.
- The Platform CLI and sidecar should come from a release artifact for the
  operator's operating system whenever possible. See
  [CLI Platform Support](../guides/cli-platform-support.md).

Supported operator platforms are Linux x86_64, Linux ARM64, macOS Apple Silicon,
macOS Intel, and Windows x86_64. Windows ARM64 is a secondary target until the
release pipeline has validated artifacts for it.

## Verify the sidecar binary (recommended)

Before running `agentforge-sidecar` from a release artifact, verify its
Sigstore signature to confirm it was built and signed by the official release
pipeline and has not been tampered with.

**Prerequisites:**

1. Install `cosign` (one-time, any platform):
   <https://docs.sigstore.dev/cosign/installation/>
2. Download the artifact **and** its companion `.sig.bundle` from the same
   GitHub release page. Both files must match.

**Verify:**

For macOS or Linux:

```bash
agentforge verify --tag v1.2.3 ./agentforge-sidecar-linux-amd64
```

For Windows (PowerShell):

```powershell
agentforge verify --tag v1.2.3 .\agentforge-sidecar-windows-amd64.exe
```

Successful output:

```
verified: ./agentforge-sidecar-linux-amd64 (tag v1.2.3, repo Wisdoverse/Wisdoverse-Forge)
```

**If verification fails:** Do not run the artifact. Re-download both the
artifact and its `.sig.bundle` from the official release page at
`https://github.com/Wisdoverse/Wisdoverse-Forge/releases/tag/v1.2.3` and
retry. If it still fails, file a security issue before using the binary.

Each release also publishes an SBOM as `<artifact>.sbom.json` (CycloneDX
format) for downstream supply-chain audit. Download it from the same release
page if you need it for your compliance workflow.

## Verify a container image (recommended for self-hosted deployments)

If you run the platform from the published `ghcr.io` container images instead
of (or alongside) the standalone binaries, verify each image before deploying
it. The images are the primary shipped artifacts for the container runtime and
are signed with Sigstore keyless cosign **by digest** by the official
`publish-images.yml` workflow. Each also carries a SLSA build-provenance
attestation.

**Prerequisites:**

1. Install `cosign` (one-time, any platform):
   <https://docs.sigstore.dev/cosign/installation/>
2. Know the image you want to run. The security-critical one is the sidecar
   (`ghcr.io/wisdoverse/wisdoverse-forge/sidecar`); the same path works for
   `server`, `orchestrator`, the root frontend image, `agent-base`, and the
   `agent-<tool>` images.

**Verify by digest (recommended — a digest is immutable):**

```bash
agentforge verify-image ghcr.io/wisdoverse/wisdoverse-forge/sidecar@sha256:<digest>
```

You can also verify a moving tag; cosign resolves the tag to its current digest
before checking the signature:

```bash
agentforge verify-image ghcr.io/wisdoverse/wisdoverse-forge/sidecar:main
```

To pin the signing identity to one release tag (rejects any other ref):

```bash
agentforge verify-image \
  ghcr.io/wisdoverse/wisdoverse-forge/sidecar:v1.2.3 \
  --ref refs/tags/v1.2.3
```

Successful output:

```
verified: ghcr.io/wisdoverse/wisdoverse-forge/sidecar@sha256:<digest> (signed by .github/workflows/publish-images.yml in Wisdoverse/Wisdoverse-Forge)
```

`agentforge verify-image` fails closed: it accepts a signature **only** from
the official `publish-images.yml` workflow in the official repository, not "any
Sigstore signature". An image signed by a fork, a different workflow, or a
different repository fails even if the bytes are identical.

**If verification fails:** Do not run the image. Re-pull it from
`ghcr.io/wisdoverse/wisdoverse-forge/...`, confirm the registry and path are
correct, and retry. If it still fails, file a security issue before deploying.

**SLSA provenance (optional, deeper audit):** Each signed image also has a
build-provenance attestation recording which workflow, commit, and runner built
it. Inspect it with the GitHub CLI:

```bash
gh attestation verify oci://ghcr.io/wisdoverse/wisdoverse-forge/sidecar@sha256:<digest> \
  --repo Wisdoverse/Wisdoverse-Forge
```

## Enroll

Run this from the local directory where the agent should work. Replace
`<project-id>` with the project selected in the web UI.

For macOS or Linux Terminal:

```bash
agentforge agents enroll-local \
  --tool codex \
  --name "Host Codex" \
  --project <project-id> \
  --cwd "$PWD" \
  --shell-format bash
```

For Windows PowerShell:

```powershell
agentforge agents enroll-local `
  --tool codex `
  --name "Host Codex" `
  --project <project-id> `
  --cwd "$($PWD.Path)" `
  --shell-format powershell
```

The command returns the environment variables plus `agentforge-sidecar` in the
selected shell syntax. Run that returned block in the local directory that
should receive tasks. The platform will then see heartbeats, assign tasks
through NATS, and receive signed result evidence from that sidecar.

The `agentforge agents enroll-local` command generates an `Idempotency-Key`
header automatically. If you re-run the same command within 24 hours, the
platform returns the same agent rather than creating a duplicate. To force a
fresh enrollment, pass `--idempotency-key <new-uuid>` or wait for the 24-hour
window to expire.

## Verify

After the sidecar starts:

1. Open the Wisdoverse Forge web UI.
2. Go to Agents.
3. Confirm the new agent appears as `Host CLI`.
4. Confirm the agent status changes after the first heartbeat.
5. Assign a small task and check that task updates and evidence return to the
   platform.
6. (Optional) Confirm the platform recorded the enrollment as Host CLI:
   - Web UI: the agent's detail page shows the "Host CLI" badge.
   - DB: `SELECT runtime_kind FROM agents WHERE id = '<agent-id>';` returns 'cli'.
   - Audit: `SELECT * FROM events WHERE agent_id = '<agent-id>' AND event_type = 'agent.enrolled';`
     should return one row with your user_id and source IP.

If the agent stays offline, check that the local machine can reach the API and
`NATS_AGENT_URL`, then rerun the sidecar command from the enrollment output.

## Network

Host CLI enrollment requires `NATS_AGENT_URL` to use TLS (`tls://`) by default.
If your deployment runs NATS without TLS (lab/sandbox only), an organization
admin must set the policy flag `allow_plaintext_host_nats = true` before
enrollment will succeed. Production deployments should never set this flag.

## Revoke

Delete the managed agent row from the platform, or clear its enrollment by
removing the agent. The per-agent NATS password and HMAC key are stored only on
the agent row and are no longer valid after that row is removed.

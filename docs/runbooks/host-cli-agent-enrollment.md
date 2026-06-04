# Host CLI Agent Enrollment

Use this when an operator wants a CLI agent running on their own machine to join
the remote Wisdoverse Forge control plane.

This runbook is written for operators who may not be professional engineers.
Follow the steps in order. Advanced runtime details are intentionally kept after
the basic path.

## What this does

A Host CLI agent is a normal managed Agent that runs from your own computer
instead of inside a platform container. The platform still creates the Agent,
assigns work, receives heartbeats, and records evidence. Your local terminal
runs the selected work tool and the sidecar process that connects it back to the
platform.

Use this path when:

- You want your local `codex`, `claude`, `gemini`, or `opencode` CLI to receive
  platform tasks.
- The local machine can reach the remote Forge server.
- You can keep one terminal window open while the Agent is working.

Do not use this path for API/provider agents or container-backed Agents created
entirely inside the platform.

## Before you start

Operator prerequisites:

- You can open the Forge web UI and sign in.
- You have a Platform CLI token from an owner or admin.
- You know which project should receive the Agent.
- You have a local folder where the Agent is allowed to work.
- The selected work tool is installed locally: `codex`, `claude`, `gemini`, or
  `opencode`.
- `agentforge` and `agentforge-sidecar` are installed from the release artifact
  for your operating system when possible. See
  [CLI Platform Support](../guides/cli-platform-support.md).

Admin prerequisites:

- The remote API is reachable from the operator's machine.
- Host CLI enrollment is enabled for the organization.
- `NATS_AGENT_URL` points to a NATS address reachable from the operator's
  machine. The API removes any user-info and injects per-agent credentials.

Supported operator platforms are Linux x86_64, Linux ARM64, macOS Apple Silicon,
macOS Intel, and Windows x86_64. Windows ARM64 is a secondary target until the
release pipeline has validated artifacts for it.

## Short path

1. Install and verify `agentforge` plus `agentforge-sidecar`.
2. Point `agentforge` at the Forge server.
3. Log in with your platform token.
4. Open the local folder where the Agent should work.
5. Run the enrollment command for your shell.
6. Copy and run the launch block printed by the command.
7. Confirm the Agent appears online in the web UI.

### Connect the CLI to Forge

Replace `https://forge.example.com` and `<platform-token>` with values from your
owner or admin.

For macOS or Linux Terminal:

```bash
agentforge config set server https://forge.example.com
agentforge auth login --token <platform-token>
agentforge auth status
```

For Windows PowerShell:

```powershell
agentforge config set server https://forge.example.com
agentforge auth login --token <platform-token>
agentforge auth status
```

Success looks like:

- `agentforge auth status` says you are signed in.
- The server URL matches your Forge server.
- No token value is printed back to the terminal.

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

## Enroll the local Agent

Run this from the local directory where the Agent should work. Replace
`<project-id>` with the project ID from the Forge web UI.

Where to find `<project-id>`:

1. Open the project in the web UI.
2. Copy the project ID from the project settings page or the URL if your
   deployment exposes it there.
3. Ask an owner or admin if you only see the project name.

The examples below use `codex`. Replace it with `claude`, `gemini`, or
`opencode` if that is the local work tool installed on your machine.

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

The command prints a launch block for your shell. It contains several
environment variables followed by `agentforge-sidecar`. Copy the entire block
and run it in the same local folder.

Success looks like:

- The terminal keeps running instead of immediately returning to the prompt.
- The sidecar logs show that it connected to the platform.
- The Agents page shows the new Agent as online after the first heartbeat.
- Tasks assigned to that Agent create updates and evidence in the platform.

Keep that terminal open while the Agent is working. Closing it stops the local
Agent connection; the platform can keep the Agent record, but it cannot assign
new work to the local machine until you run the launch block again.

The `agentforge agents enroll-local` command generates the required
`Idempotency-Key` header automatically from the command inputs. If the network
drops and you run the same command again within the platform's 24-hour replay
window, the platform can return the same Agent instead of creating a duplicate.
You do not need to copy, remember, or type that header yourself.

To intentionally create another local Agent, use a different `--name` and run
the enrollment command again after confirming the first Agent is no longer
needed, or wait until the replay window expires.

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

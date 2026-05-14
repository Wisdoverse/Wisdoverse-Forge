# Docker Assets for Wisdoverse Forge

This directory contains the Docker and Compose assets for the current Rust-first runtime. The default Compose path deploys the Rust API, Rust orchestrator, Temporal, and supporting infrastructure. Legacy helper services are not part of the default path.

For the operator guide, see [docs/guides/deployment.md](../docs/guides/deployment.md).

## Quick Start

```bash
npm install
make quickstart-local
npm run dev
```

For local UI development, keep `npm run dev` running separately from the backend stack.

For the self-contained production profile:

```bash
make quickstart-selfhost-pull DOMAIN=forge.example.com
```

Use the default `DOMAIN=localhost` for a private trial. Public domains receive
automatic HTTPS through the Caddy service in the `prod` profile.

When the host already has a web server on `80` or `443`, use alternate public
ports:

```bash
make quickstart-selfhost-pull DOMAIN=localhost HTTP_PORT=18080 HTTPS_PORT=18443
```

Use `make quickstart-selfhost` when you intentionally want to build the server
and frontend images from source on the host.

Run `make beginner-audit BEGINNER_AUDIT_FLAGS="--pull-images --local-smoke"`
to verify the prebuilt-image path with an isolated localhost production stack.
For CDN-backed domains, add `BEGINNER_ORIGIN_IP=<vps-ip>` to the `--live`
audit so the source host is checked directly on `:80` and `:443`.
After a provider is already configured and tested in Settings, add
`BEGINNER_USE_EXISTING_PROVIDER=1` to the `--provider` audit to reuse it without
re-entering the API key.
For local keyless inference, run Ollama on the host or another reachable
machine, set `OLLAMA_BASE_URL` in `docker/.env`, then add the `ollama` provider
without an API key.

## Agent Images

Container CLI sessions use local image tags such as `agentforge-agent:codex`.
For public releases, pre-built images are published to GitHub Container
Registry under `ghcr.io/wisdoverse/wisdoverse-forge`.

```bash
make update-agents
```

That command pulls `agent-base` using `GHCR_IMAGE_TAG` (`main` by default) and
the redistributable public CLI images (`agent-opencode`, `agent-codex`, and
`agent-gemini`), then tags them locally for the Rust API. Public releases
intentionally do not publish `agent-claude`
because Claude Code is governed by Anthropic terms rather than a standard
open-source redistribution license. To use Claude after accepting the vendor
terms, or to test local runtime or sidecar changes, build the image locally:

```bash
make build-agent-all
# or only Claude:
make build-agent CLI_TOOL=claude
```

Private deployments may publish a Claude image to an internal registry only when
their third-party terms permit redistribution:

```bash
make update-agents AGENT_REGISTRY=registry.example.com/wisdoverse/forge AGENT_TOOLS="claude opencode codex gemini"
```

See [Third-Party CLI Image Policy](../docs/security/third-party-cli-images.md)
for the public redistribution boundary.

## Key Files

| File                   | Purpose                             |
| ---------------------- | ----------------------------------- |
| `compose.yml`          | Base service definitions            |
| `compose.dev.yml`      | Development overlay                 |
| `compose.prod.yml`     | Self-contained production overlay   |
| `compose.external.yml` | External-service production overlay |
| `.env.example`         | Docker configuration template       |
| `Dockerfile.agent`     | Agent image build                   |
| `deploy-frontend.sh`   | Static frontend deployment helper   |
| `prometheus/`          | Prometheus examples and alert rules |
| `alertmanager/`        | Secretless Alertmanager examples    |

## Default Services

| Service                 | Role                                   |
| ----------------------- | -------------------------------------- |
| `agentforge-server`     | Rust API and realtime gateway          |
| `agentforge-frontend`   | Built SPA artifact server in `prod`    |
| `orchestrator`          | Rust orchestrator and workflow runtime |
| `temporal`              | Workflow engine                        |
| `db`, `orchestrator-db` | Persistence layers                     |
| `redis`, `nats`         | Coordination and event transport       |
| `caddy`                 | Reverse proxy and automatic HTTPS      |

## Notes

- `make dev` starts the backend platform, not the Vite frontend.
- `make prod` serves the built frontend through the `agentforge-frontend`
  artifact service and Caddy. The `external` profile deploys frontend assets
  separately through the configured web tier.
- The agent base image installs a Rust-built `docker-buildx` proxy from `rust/bins/buildx-plugin/` to intercept `docker buildx build` inside agent containers.
- The Alertmanager examples are wiring templates only. Keep webhook URLs, tokens,
  and contact-point secrets in the deployment secret store, then generate a
  sanitized route contract with `scripts/release/orchestration_alert_route_contract.mjs`.
- If you change the default runtime path, update this file and `docs/guides/deployment.md` together.

# Third-Party CLI Image Policy

Wisdoverse Forge agent images can include third-party Container CLI tools. Public
release images must include only tools whose package license permits public
redistribution under standard open-source terms.

## Public GHCR Image Set

The public GitHub Container Registry release publishes:

- `agent-base`
- `agent-opencode`
- `agent-codex`
- `agent-gemini`

These images are built with pinned CLI package versions resolved during release
and include BuildKit SBOM and provenance attestations.

## Claude Code

Public releases do not publish `agent-claude`. The Claude Code package license
points to Anthropic legal terms instead of a standard open-source
redistribution license.

Operators who need Claude Code should build it locally after accepting the
applicable vendor terms:

```bash
make build-agent CLI_TOOL=claude
```

Private deployments may publish a Claude image to an internal registry only when
their Anthropic terms permit that redistribution:

```bash
make update-agents AGENT_REGISTRY=registry.example.com/wisdoverse/forge AGENT_TOOLS="claude opencode codex gemini"
```

## Release Guardrails

- Keep `@anthropic-ai/claude-code` out of the public GitHub Actions release
  matrix unless legal approval explicitly confirms public redistribution rights.
- Keep public release notes clear that third-party CLI names and trademarks
  belong to their respective owners.
- Keep SBOM/provenance enabled for public agent images.

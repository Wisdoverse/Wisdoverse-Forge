# Support

Thanks for using Wisdoverse Forge. This page explains where to get help so your
question reaches the right place quickly.

## Before you ask

Most setup questions are answered in the docs:

- [Getting Started](docs/guides/getting-started.md) — first-run, local trial, and self-host paths.
- [Documentation map](docs/README.md) — architecture, guides, runbooks, and the truth hierarchy.
- [Runbooks](docs/runbooks/) — operational procedures (runtime validation, observability, enrollment, migrations).
- [Configuration guide](docs/guides/configuration.md) — runtime configuration and environment variables.

The project is an engineering preview for trusted self-hosted environments, so
start from the local trial in the [README](README.md) before deploying.

## Where to go

| You want to… | Use |
| --- | --- |
| Report a reproducible bug | [Open a Bug report issue](https://github.com/Wisdoverse/Wisdoverse-Forge/issues/new?template=bug_report.yml) |
| Propose a feature or change | [Open a Feature request issue](https://github.com/Wisdoverse/Wisdoverse-Forge/issues/new?template=feature_request.yml) |
| Ask a usage or "how do I…" question | Search [existing issues](https://github.com/Wisdoverse/Wisdoverse-Forge/issues), then open a question issue if it is not covered |
| Report a security vulnerability | **Do not open a public issue.** Use [private Security Advisories](https://github.com/Wisdoverse/Wisdoverse-Forge/security/advisories/new) — see [SECURITY.md](SECURITY.md) |
| Contribute a change | Read [CONTRIBUTING.md](CONTRIBUTING.md) and the [DDD](docs/architecture/ddd-contract.md) / FSD contracts first |

## Good questions include

- What you expected versus what happened.
- The exact commands you ran and their output (redact secrets, tokens, and hostnames).
- Your deployment mode (local trial, self-hosted, or development) and the commit or release tag.
- OS, Docker / Docker Compose version, and Node.js version.

This helps maintainers reproduce and answer without a round-trip.

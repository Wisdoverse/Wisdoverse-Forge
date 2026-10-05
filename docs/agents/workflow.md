# Workflow And Validation

Read the relevant sections before branch work, validation, PR work, or runtime changes.

## Branches

- Follow the [work protection rules](../../AGENTS.md#protect-work-and-privacy).
- Unless the user selected another base, rebase or merge against current `origin/main` before a PR push.
- Push the branch for requested PR work.
- Create or update the PR with concrete validation evidence.

## Validation

Choose checks from the changed behavior and its dependencies.

| Change | Required checks |
| --- | --- |
| Documentation or agent instructions | `git diff --check` |
| Frontend or shared TypeScript | `npm run fsd:check`, `npm run lint`, `npm run format:check`, `npm run typecheck`, relevant Vitest project |
| Rust | Narrow Rust test first |
| Shared Rust crates, API contracts, orchestration, auth, DB, or platform security | Narrow Rust test, then `make -C rust ci` |
| Protocol definitions | `npm run proto:gen`, then `npm run proto:check` |
| Runtime or deployment | Relevant Compose target and service health checks |
| Production contracts | `make prod-ext`, service health checks, and orchestration-chain checks |

- Commit generated protocol files with their source changes.
- Use `npm run test:unit` or `npm run test:integration` for the relevant Vitest project.
- Use `npm run test:e2e` for browser journeys affected by the change.
- Report each check's actual result.
- Identify checks blocked by the environment.

`make -C rust ci` runs format, Clippy, workspace tests, and dependency audit checks.
The [dependency policy](../security/dependency-policy.md) defines audit requirements.
`package.json`, `Makefile`, and `rust/Makefile` contain the current commands.

## Pull Requests

- Use `gh` for GitHub and `glab` for GitLab.
- If CLI flags differ, check the command's `--help` output.
- Use the provider API when necessary.
- For GitHub queues, take one snapshot with `npm run pr:summary`.
- For GitLab queues, use `glab mr view --output json` or a field-limited `glab api` call.
- Report one compact state, its classification, and the next useful action.

| State | Meaning | Action |
| --- | --- | --- |
| `ACTION` | Failure, cancellation, manual job, conflict, requested changes, or a blocker with a concrete fix | Fix the blocker |
| `WAIT` | CI queued or running, review pending, or server-side merge pending | Enable permitted auto-merge, then stop |
| `DONE` | PR merged, intentionally closed, or the requested pipeline completed | Report the result |

- For `WAIT`, stop repeated status reads in the chat.
- Distinguish pipeline completion from PR merge.
- To display the same snapshot again, use `npm run pr:summary:local`.
- For authorized external monitoring, schedule `npm run pr:summary:monitor`.
- Fetch logs only after a failed terminal state.
- Fetch only the failed job list and a short trace tail.
- Unless the user requests a live watch, avoid watch commands, polling loops, and repeated forced refreshes.
- Keep the script's repeat-read guard at 60 seconds or more.
- Do not store emergency bypass flags in reusable commands.
- If a bounded local waiter is necessary, limit its output to terminal results.
- Stop that waiter before the final response.

The monitor command reuses recent snapshots and alerts only when a PR requires action.
Watch commands include `gh pr checks --watch`, `gh run watch`, and `glab pipeline ci trace --watch`.

## Compose And Agent Images

`docker/compose.yml` is the canonical Compose file.
Environment-specific files are thin overrides.
Profiles include `dev`, `prod`, `external`, `tools`, `backup`, `storage`, and `casdoor`.

1. Before first local Compose use, run `make setup`.
2. Choose the command for the required environment.

Setup creates required external networks and configures OAuth mount permissions.

| Purpose | Command |
| --- | --- |
| Development backend | `make dev` |
| Development browser, in a separate terminal | `npm run dev` |
| Rust API without Compose | `npm run server` |
| Database migration without Compose | `npm run migrate` |
| Production with bundled services | `make prod` |
| Production with external services | `make prod-ext` |
| External production logs | `make prod-ext-logs` |
| Stop external production | `make prod-ext-down` |
| Agent base image | `make build-agent-base` |
| All supported Container CLI images | `make build-agent-all` |

`make prod-ext` combines `docker/compose.yml` with `docker/compose.external.yml`.
It reads external service settings from `docker/.env`.
`docker/Dockerfile.agent-base` contains system dependencies, the sidecar, and platform CLIs.
`docker/Dockerfile.agent` adds the selected Container CLI.

- After sidecar, system dependency, or platform CLI changes, rebuild the base image.
- Read [backend security rules](backend.md#security) before container runtime changes.
- Before debugging external production, inspect `docker/.env` locally.
- Keep its secrets and private deployment details out of public artifacts.
- Run `make prod-ext` for the production validation path.
- Before editing code, check API, orchestrator, NATS, Temporal, and service logs.
- Use the [deployment guide](../guides/deployment.md) for topology and prerequisites.

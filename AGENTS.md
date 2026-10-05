# Wisdoverse Forge Agent Instructions

Wisdoverse Forge is a self-hosted AI workbench for teams.
It uses a Rust control plane, agent runtimes, Temporal workflows, and a React browser app.

This file is the canonical agent entry point.
`CLAUDE.md` is a symlink to this file.
Keep the symlink intact.
Keep this file at 80 lines or fewer.
Put detailed instructions in the task guides below.

## Start Here

1. Inspect `git status --short --branch` before edits, commits, rebases, or pushes.
2. Read the guide sections that match the task.
3. Use current source files, tests, and command output to resolve stale documentation.
4. Choose checks from the [validation table](docs/agents/workflow.md#validation).

| Task | Required guide |
| --- | --- |
| Branches, checks, PRs, Compose, or agent images | [Workflow](docs/agents/workflow.md) |
| Rust, APIs, data, auth, security, or agent execution | [Backend](docs/agents/backend.md) |
| React, browser behavior, or TypeScript contracts | [Frontend](docs/agents/frontend.md) |
| Instructions, documentation, UI copy, or reports | [Writing](docs/agents/writing.md) |

For changes across these areas, read each applicable guide.
For CLI changes, also read [platform support](docs/guides/cli-platform-support.md).
For dependencies, also read the [security policy](docs/security/dependency-policy.md).

## Protect Work And Privacy

- Use a separate worktree for PR-sized changes or concurrent work.
- Base the worktree on the current target branch.
- Keep the primary checkout clean.
- Leave unrelated user changes intact.
- Stage only files for the requested change.
- Keep commits within the requested scope.
- Treat this repository and its review artifacts as public.
- Exclude secrets, private hostnames, internal URLs, real operator emails, and private organization details from all artifacts.
- Apply this restriction to code, configuration, commits, PRs, issues, documentation, instructions, and memory files.
- Use `staging.example.com`, `gitlab.example.com`, and `dev@example.com` as placeholders.
- Read deployment targets from repository secrets in workflows.

## Preserve Runtime Boundaries

- Add backend behavior in `rust/`.
- Keep active frontend code under `src/app`.
- Preserve authentication and organization isolation.
- Use `agents.workspace_id` as the execution and access boundary.
- Treat `agents.project_id` as primary UI context and task routing only.
- Permit cross-project access only within the same workspace and organization.

PostgreSQL is required.
Redis and NATS can degrade only where the code explicitly supports degradation.
The [architecture overview](docs/architecture/overview.md) describes runtime topology.
The [repository map](README.md#repository-map-for-agents) identifies active paths and ownership.

## Delegate Simple Tasks

- Use `gpt-6-luna` for simple, independent tasks with a clear result.
- Suitable tasks include rule inventories, link checks, and small documentation corrections.
- Give each task a limited scope, an expected result, and a validation command.
- Keep concurrent agents from editing the same files.
- Keep architecture, auth, security, ambiguous changes, and final review with the primary agent.
- If Luna is unavailable, complete the task with the available model.

## Write And Finish

- Apply the strict project profile in the [writing guide](docs/agents/writing.md).
- Use one instruction per sentence.
- Limit instruction sentences to 20 words and descriptive sentences to 25 words.
- Preserve exact code identifiers and external protocol names.
- Report the result, validation evidence, and unresolved blockers.
- Distinguish local checks, remote CI, review approval, and merge status.
- For PR work, follow the [queue rules](docs/agents/workflow.md#pull-requests).

Start local setup with the [README](README.md).
Follow [CONTRIBUTING.md](CONTRIBUTING.md) for contribution requirements.

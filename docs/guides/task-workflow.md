# Task Workflow Guide

This guide describes the current product workflow for turning setup into useful
agent work. It is written for operators and product reviewers who need to verify
the browser flow, not for backend-only API testing.

## Scope

The primary workflow is:

1. Prepare workspace routing.
2. Confirm runtime readiness.
3. Add a provider or Container CLI credential.
4. Create an agent.
5. Create and assign a task.
6. Review execution, artifacts, evidence, and context.
7. Save reusable learning as a skill draft when completed work is durable.

## Prerequisites

- The Rust API is reachable on `:4003`.
- The browser app is reachable on `:4002` in development, or through the
  production frontend service.
- The user has an organization context and can create or select a team, project,
  task group, provider, and agent.
- Provider-backed agents need a tested provider in Settings -> Providers.
- Container CLI agents need a configured runtime image and a connected CLI
  credential or deployment-level fallback key.

## First-Run Checklist

Open `/start` after login. The checklist is the shortest path through the
product:

| Step                   | Route                                        | Expected result                                                                                     |
| ---------------------- | -------------------------------------------- | --------------------------------------------------------------------------------------------------- |
| Workspace              | Settings -> Projects                         | A team and project exist for task routing.                                                          |
| Runtime                | Settings -> Runtime                          | At least one runtime and one Container CLI option are available.                                    |
| Provider or credential | Settings -> Providers or Settings -> Runtime | Provider-backed agents have a tested provider, or Container CLI agents have a connected credential. |
| Agent                  | Agents                                       | An agent exists with a runtime, model or CLI tool, and project context.                             |
| First task             | Tasks                                        | A task is created in the selected project task group.                                               |
| Review                 | Tasks -> task detail                         | The Work, Result, Context, and Updates tabs show the task lifecycle.                                |
| Reusable learning      | Skills or Context                            | Completed work can be reviewed and drafted as a skill.                                              |

## Runtime Readiness

Use Settings -> Runtime before creating Container CLI agents. The page combines
signals from several backend surfaces:

- Runtime defaults from `GET /api/v1/settings/runtime`.
- CLI image metadata from the `cliToolDetails` response field.
- CLI credential state from `GET /api/v1/cli-auth-proxy/status`.
- Agent heartbeat state from `GET /api/v1/orchestration/participants`.

The readiness panel should show:

- Default runtime.
- CLI image/version reporting.
- Connected credential count.
- Latest heartbeat source.
- Remediation guidance when runtime settings, CLI images, credential state, or
  participant heartbeats are missing.

The Rust settings API reports each CLI tool as:

```json
{
  "cliTool": "claude",
  "image": "agentforge-agent:claude",
  "version": "claude",
  "imagePresent": false,
  "versionSource": "image-tag"
}
```

When Docker can inspect the image, `imagePresent` is true and `versionSource`
is either `docker-label` or `image-tag`.

## Task Creation And Assignment

Open Tasks after selecting a project. The board uses project-scoped task groups.
If no project or group exists, the empty state should point the user to the
owning setup action instead of requiring manual API work.

Before a task is created, the board shows assignment readiness:

- Available agents can take new work.
- Busy or offline agents are visible but disabled in the task form.
- If no online agent is available, a task can still be created and queued for
  later dispatch.

Task creation should capture the title, prompt body, project task group,
priority, and optional assigned agent. Tasks that require unavailable inputs or
approval may start blocked instead of immediately dispatching.

## Task Review

Open a task from the board to review the work. The detail panel is the primary
review surface:

| Tab     | Purpose                                                                                   |
| ------- | ----------------------------------------------------------------------------------------- |
| Work    | Brief, assignment, execution state, artifacts/evidence summary, and final review actions. |
| Result  | Completed task artifacts rendered from the task result payload.                           |
| Context | Context candidates and injection state when governance or injection features are enabled. |
| Updates | Execution attempts from task runs plus lifecycle status.                                  |

The Updates tab reads real execution attempts from:

```text
GET /api/v1/orchestration/tasks/{id}/runs
```

Use it to distinguish a queued task, a running attempt, a completed attempt, and
a failed retry path.

## Reusable Learning

When a task is completed, the Work tab can open the skill draft flow. The draft
is prefilled from the task title, task body, result artifacts, and evidence or
context hints. Review the draft before publishing so transient task details do
not become reusable instructions.

After publishing, attach the skill to the agents that should reuse it. The
agent profile shows recent task activity and applied skill counts so reviewers
can confirm whether the agent is actually using reusable learning.

## Validation

For changes to this workflow, run checks by the changed surface:

```bash
npm run fsd:check
npm run lint
npm run format:check
npm run typecheck
npm run test:unit -- --run tests/unit/app/GettingStartedView.test.tsx tests/unit/app/BoardView.test.tsx tests/unit/app/TaskDetailPanel.test.tsx tests/unit/app/AgentDetailView.test.tsx
```

For Rust API changes in runtime settings or orchestration task runs, also run:

```bash
cd rust
cargo test -p agentforge-api --lib routes::settings
cargo test -p agentforge-api --lib domain::orchestration
```

Workspace-wide Rust tests that touch SQLx test databases require `DATABASE_URL`
to be set.

# Product UX Direction

Wisdoverse Forge should keep its governed, auditable execution model, but the
primary user experience needs a shorter path from setup to useful agent work.
The product should feel less like a collection of infrastructure surfaces and
more like a team workspace where agents can be assigned work, report progress,
and leave reusable knowledge behind.

## Product Contract

- Make the first-run path explicit: connect a runtime, add a provider or
  credential, create an agent, create a task, assign it, watch progress, review
  output, and save reusable learning.
- Treat the task board as the primary work surface. Task details should combine
  description, assignment, comments or updates, execution log, result artifacts,
  evidence, context, and final review in one coherent flow.
- Make agents feel like managed teammates, not only runtime resources. Agent
  pages should foreground presence, current work, skills, recent updates,
  availability, and how the agent can be assigned.
- Promote runtimes to first-class setup and operations surfaces. Users should
  see runtime health, available Container CLIs, versions, last heartbeat,
  credential state, and direct remediation guidance.
- Turn context, evidence, governance, and skills into progressive capabilities
  inside the task flow. New users should not have to understand every governance
  primitive before assigning the first task.
- Close the skill reuse loop. Completed work should offer a clear path to
  extract a reusable skill, attach it to agents, and see when it is used later.
- Keep workspace/project/group concepts visible only when they help routing or
  permissions. The default workflow should hide unnecessary hierarchy until the
  team needs it.

## Feature UX Acceptance Checklist

Every new or changed product surface must pass this checklist before it is
ready for review. This applies to browser UI, Platform CLI commands, runbooks,
API-facing errors, and operator-facing automation.

| Requirement              | Acceptance evidence                                                                                                                                                                                           |
| ------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Shortest safe path       | The surface states the next safe action before exposing advanced options. If setup is incomplete, the first action sends the user to the missing prerequisite.                                                |
| Plain-language outcome   | Titles, badges, empty states, and button labels describe what the user can do or what changed, not only the internal object name. Prefer "Ready to assign" over "idle" when the screen is about task routing. |
| Visible prerequisites    | Required project, runtime, provider, credential, agent, task group, or permission state is shown before the user submits work. Missing prerequisites include one direct action and a success condition.       |
| Clear success state      | The user can tell what success looks like after completing the action. For async work, show the next place to watch progress or verify evidence.                                                              |
| Recoverable errors       | Errors explain the failed action, avoid leaking internal details, and include a retry, refresh, setup, or navigation path when one exists.                                                                    |
| Safe destructive actions | Delete, revoke, rotate, and permission-changing actions require explicit confirmation that names the affected resource and the expected impact.                                                               |
| Progressive detail       | The first screen uses simple decisions. Advanced IDs, raw runtime names, logs, and diagnostics stay in details, troubleshooting, or copyable evidence blocks.                                                 |
| Cross-platform CLI path  | CLI-facing features document copy-pasteable Linux, macOS, and Windows operator paths when the action can run locally. Follow [CLI Platform Support](../guides/cli-platform-support.md).                       |
| Testable operator path   | PR validation names the user path checked, not only the technical command. Tests or screenshots should cover empty, loading, success, and recovery states when the feature owns them.                         |

## Current Implemented Surface

| Surface               | Current behavior                                                                                                                                                                                                                                                                |
| --------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| First-run checklist   | `/start` walks through workspace, runtime, provider or CLI credential, agent creation, first task, review, and reusable learning. Each step links to the owning app route.                                                                                                      |
| Runtime readiness     | Settings -> Runtime shows runtime options, available Container CLIs, image/version reporting, CLI credential state, latest agent heartbeat, refresh, and remediation actions. The Rust settings API returns `cliToolDetails` with image, presence, version, and version source. |
| Task board assignment | The task board shows assignment readiness before task creation. Task creation surfaces available agents, disables busy or offline agents, and explains when work will queue until an agent is available.                                                                        |
| Task detail review    | Task details use Work, Result, Context, and Updates tabs. The Work tab combines brief, assignment, execution log, artifacts/evidence, reusable learning, and completion review.                                                                                                 |
| Execution history     | The Updates tab reads task run attempts through `GET /api/v1/orchestration/tasks/{id}/runs` and combines them with task lifecycle state.                                                                                                                                        |
| Agent profile         | Agent detail pages show assignment fit, runtime mode, credential guidance, current or recent task activity, and applied skill counts derived from recent task data.                                                                                                             |
| Skill reuse           | Completed tasks can open a draft skill review path. The draft is prefilled from task result artifacts and task context before publishing through the skills store.                                                                                                              |

## Current Flow

1. Open `/start`.
2. Confirm workspace, team, and project routing.
3. Confirm runtime readiness in Settings -> Runtime.
4. Add and test a provider in Settings -> Providers, or connect a Container CLI credential.
5. Create an agent from Agents.
6. Open Tasks, create a task, and assign it to an available agent or leave it queued.
7. Review task progress from the detail panel.
8. On completion, review artifacts/evidence and draft a reusable skill when the output contains durable knowledge.

See [Task Workflow Guide](../guides/task-workflow.md) for the operator-facing version of this flow.

## Remaining Product Gaps

- Human comments and blocker updates should become first-class task records,
  separate from execution attempts and lifecycle state.
- The save-as-skill path creates a draft, but attaching that skill back to
  matching agents should be a clearer next action.
- Runtime readiness is visible in Settings; a future operations view can combine
  runtime, provider, participant, and queue state for incident triage.
- Empty states should continue to prefer direct actions over conceptual
  explanations when no project, task group, provider, runtime, agent, or
  available participant exists.

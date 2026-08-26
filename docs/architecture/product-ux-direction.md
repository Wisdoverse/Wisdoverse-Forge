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

| Surface                      | Current behavior                                                                                                                                                                                                                                                                                                            |
| ---------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Root landing                 | `/` routes new workspaces to the `/start` checklist and returning users to `/tasks`, using the per-user "get started dismissed" preference (skip or complete flips it). If preferences cannot load, `/` falls back to the board.                                                                                            |
| Human block on board         | The board fetches the latest blocker/unblock mark per task (`GET /api/v1/orchestration/tasks/comments/latest`) and shows a red "Blocked by a person" badge with the author and body on hover. The `n` key opens the New Task flow.                                                                                          |
| First-run checklist          | `/start` walks through workspace, runtime, provider or CLI credential, agent creation, first task, review, and reusable learning. Each step links to the owning app route.                                                                                                                                                  |
| Runtime readiness            | Settings -> Runtime shows runtime options, available Container CLIs, image/version reporting, CLI credential state, latest agent heartbeat, refresh, and remediation actions. The Rust settings API returns `cliToolDetails` with image, presence, version, and version source.                                             |
| Task board assignment        | The task board shows assignment readiness before task creation. Task creation surfaces available agents, disables busy or offline agents, and explains when work will queue until an agent is available.                                                                                                                    |
| Task detail review           | Task details use Work, Result, Context, and Updates tabs. The Work tab combines brief, assignment, execution log, artifacts/evidence, reusable learning, and completion review.                                                                                                                                             |
| Human updates                | Human notes and blocker signals are first-class task records (GET                                                                                                                                                                                                                                                           | POST /api/v1/orchestration/tasks/{id}/comments), shown on the task Updates tab with a Note / Block / Unblock composer and author-only delete.                                                                                          |
| Execution history            | The Updates tab reads task run attempts through `GET /api/v1/orchestration/tasks/{id}/runs` and combines them with task lifecycle state.                                                                                                                                                                                    |
| Agent profile                | Agent detail pages show assignment fit, runtime mode, credential guidance, current or recent task activity, and applied skill counts derived from recent task data.                                                                                                                                                         |
| Skill reuse                  | Completed tasks can open a draft skill review path. The draft is prefilled from task result artifacts and task context before publishing through the skills store.                                                                                                                                                          |
| Skill attach-back            | After publishing, the draft flow offers inline agent attach/detach (`GET                                                                                                                                                                                                                                                    | PUT                                                                                                                                                                                                                                    | DELETE /api/v1/skills/{id}/agents…`); the skill detail view manages the same attachment list. |
| Skill usage                  | Skill detail shows how often the skill was applied (`GET /api/v1/skills/{id}/usage`): injections, distinct runs, and last used, rendered as a quiet usage line.                                                                                                                                                             |
| Agent-side follow            | Agent profile shows the skills it follows (`GET /api/v1/agents/{id}/skills`), including revoked ones (struck-through), so "who follows this guidance" is answerable from either side.                                                                                                                                       |
| Compliance export            | The board toolbar and the governance audit page export the latest 500 tasks as CSV (`GET /api/v1/orchestration/tasks/export`): title, state, priority, creator, agent, run count, timestamps, and approval flag; the export action is itself audited.                                                                       |
| Version notification         | Settings → About runs a manual self-host update check (latest GitHub release vs installed version) with up-to-date, newer-available, and unreachable states. No background network calls; privacy-preserving by design.                                                                                                     |
| Operations overview          | `/operations` combines runtime readiness, AI services, agent availability, queue flow, and system health into one triage status (plus the enterprise sign-in mode: on with the provider name, or off); each unmet item carries one direct action. Refreshable, quiet-degrade on partial data.                               |
| Approval gate                | Task creation offers "Wait for my approval before the agent starts" (self-host-friendly guardrail); the task waits in Needs approval and the detail view's Allow and continue handles the decision with an audited record.                                                                                                  |
| Context budget               | Before publishing context, the preview sums selected item tokens against the agent's context budget and shows a usage line plus an amber/red warning when the agent would mostly or fully consume its context.                                                                                                              |
| Draft acceptance             | Auto-extraction is measured: skill draft open/publish analytics events feed a "Suggested drafts accepted" rate on the Skills page (best-effort, non-blocking).                                                                                                                                                              |
| Review checklist             | Finished tasks show a per-person review checklist (`GET                                                                                                                                                                                                                                                                     | PATCH /api/v1/orchestration/tasks/{id}/review-checks/…`): 4 evidence items (brief match, artifacts, secrets, reuse) with optimistic toggling, "X of 4" progress, and "Review complete" when all checks pass — progress saved per user. |
| Queued-time hint             | Waiting cards show "Starts in ~N min" (computed server-side from the org median task duration × real queue position, same-agent lane or shared pool); the tooltip explains the position, the typical duration basis, and how to change it. No history yet ⇒ the card honestly says it's a rough guess.                      |
| Context safety loop          | Context-overflow failures are recognized and explained: the board card says the agent ran out of context window, the task detail gives the trim-and-retry action, warning/failure events are recorded, and Analytics shows "Context safety" (warnings shown, overflow failures, % that still overflowed despite a warning). |
| SSO sign-in                  | When an OIDC provider is configured (`AUTH_SSO__*`), the login page shows "Single sign-on"; the flow is cookie-bound, single-use, and provisioned the account + team space on first sign-in (email match signs in existing members).                                                                                        |
| SSO role mapping             | With `AUTH_SSO__ROLE_CLAIM`/`AUTH_SSO__ADMIN_GROUPS`, each sign-in maps the provider's groups authoritatively onto `admin` or `member`; owners are never changed.                                                                                                                                                           |
| SSO team provisioning        | With `AUTH_SSO__TEAM_GROUP_MAP`, sign-ins also add the user to mapped teams (`teamName=group;…`) as member or admin; with deprovisioning on, losing the group removes the team membership — a renamed team is skipped instead of blocking sign-in.                                                                          |
| SSO org provisioning         | With `AUTH_SSO__ORG_GROUP_MAP`, sign-ins add the user to mapped orgs when the provider group matches (member, or admin with an admin group); `AUTH_SSO__DEPROVISION` denies access when no mapped group applies and removes other stale memberships when safe.                                                              |
| Compaction trim              | When the context preview exceeds the agent budget, "Trim to fit" suggests removing the least-recently-used items (pinned items and the whole selection are protected) down to a safe ratio; applied trims are measured and shown on the Analytics "Context safety" line.                                                    |
| Stale queue retirement       | The board toolbar offers "Retire stale tasks" (owner/admin): backlog and queued tasks untouched for 7+ days in the selected queue are batch-closed with a confirming explanation, an audited `POST /orchestration/groups/{id}/tasks/retire-stale`, and a clear result message (nothing retired when the queue is clean).    |
| Invite by email              | Settings → Team members adds "Invite by email": existing org members are added instantly; people without an account get a shareable one-time link (3-day validity). Opening it and signing up with the invited email joins the team automatically; a wrong email cannot redeem someone else's invite.                       |
| Agent reliability            | Analytics adds a "Work reliability" list (`GET /api/v1/analytics/agent-reliability?hours=`, default 30 days): each agent shows finished runs, failures and a tone-coded success-rate bar so a flaky agent is visible before a whole team blames the queue.                                                                  |
| Project-scoped templates     | A template can be limited to one project (Settings → Task templates → "Use in project"): the task form refetches when the project changes and shows that project's own templates alongside the team-wide ones.                                                                                                              |
| Wait for tasks               | The task form's Task options add "Wait for these tasks first": a task starts blocked and never dispatches until every listed prerequisite is completed; finishing a prerequisite releases dependents whose set is then fully done.                                                                                          |
| Agent usage                  | Analytics adds an "Agent usage" list (`GET /api/v1/analytics/agent-usage?hours=`, default 30 days): each agent shows assistant requests, input/output tokens and its share of the window's total tokens, so a token-hungry agent is visible before the bill gets shared with the team.                                      |
| Required review gates        | With `REVIEW_REQUIRED_GATES` set, selected review checklist items are tagged "Required" and a human cannot mark a finished task completed until every required key is ticked by a reviewer — the checklist shows pending count or an all-clear, and the refusal names exactly what is missing.                              |
| Recurring tasks              | Settings → Task templates → Recurring tasks schedules a task (name, title, project, waiting place, cadence 15 min–30 days, approval flag); the server's 60 s runner creates one unassigned task per due tick (next available agent picks it up), with pause/resume and two-click remove.                                    |
| Scheduled compliance exports | With `COMPLIANCE_EXPORT_INTERVAL_HOURS` set, the server writes a per-org CSV snapshot of the latest 1000 tasks into the configured export directory on each cadence (`<org-slug>/agentforge-compliance-<timestamp>.csv`), restart-safe via a `.last_run` marker.                                                            |
| Offline bundles              | `scripts/offline-bundle.sh --full-stack` packages the Forge images plus pinned platform services into a signed TUF bundle; `scripts/load-offline-bundle.sh` verifies trusted metadata before loading it on an air-gapped host (see `docs/guides/offline-install.md`).                                                       |
| Telemetry retention          | `ANALYTICS_RETENTION_DAYS` (0 off) purges `events` and `analytics_events` older than the window on boot and every 6 h — telemetry only; tasks, runs, comments and review evidence are never deleted by this policy.                                                                                                         |
| LLM cost estimates           | With `LLM_PRICING` set (USD per 1M tokens per model, validated at boot), Analytics' "Agent usage" rows add an estimated cost per agent (`≈ $0.04`); without rates the page shows token counts only and says exactly that.                                                                                                   |
| Task templates               | Settings → Task templates saves a reusable brief (name, task title, brief, priority, approval flag); the task form lists it under "Saved by your team" and applies title/brief/priority/approval in one click (`GET                                                                                                         | POST /api/v1/task-templates`, `DELETE /api/v1/task-templates/{id}`, delete limited to the creator or an owner/admin).                                                                                                                  |

## Current Flow

1. Sign in. `/` lands new workspaces on `/start`; returning users go to the board.
2. Confirm workspace, team, and project routing.
3. Confirm runtime readiness in Settings -> Runtime.
4. Add and test a provider in Settings -> Providers, or connect a Container CLI credential.
5. Create an agent from Agents.
6. Open Tasks, create a task, and assign it to an available agent or leave it queued.
7. Review task progress from the detail panel.
8. On completion, review artifacts/evidence and draft a reusable skill when the output contains durable knowledge.

See [Task Workflow Guide](../guides/task-workflow.md) for the operator-facing version of this flow.

## Remaining Product Gaps

Shipped since this list was written: board-level human-block signals (latest
blocker/unblock marks per task), skill usage per agent (Skills detail shows
injections, distinct runs, last use, and per-agent attachment state), and an
Operations overview page combining runtime readiness, AI services, agent
availability, queue flow, and system health for triage.

Still open, in order of user visibility:

- Per-project task templates exist for templates; project-scoped _recurring_
  tasks and project-scoped saved briefs in the task form's fill flow could be
  broader (a project-level "starter set" the form applies by default).
- Approval queue refinements: bulk approval for waiting-approval tasks and a
  count badge on the Inbox queue.
- Empty states should continue to prefer direct actions over conceptual
  explanations when no project, task group, provider, runtime, agent, or
  available participant exists.
- Operator hardening remaining: full TUF metadata (key rotation, root
  pinning), OpenTelemetry telemetry export, full SCIM schema (groups,
  attributes, paging) — see ROADMAP P5 items 2/5.

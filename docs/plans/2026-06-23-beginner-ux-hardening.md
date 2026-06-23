# Beginner UX hardening plan

Status: **In progress**
Created: 2026-06-23
Tracking issue: [#867](https://github.com/Wisdoverse/Wisdoverse-Forge/issues/867)
Branch: `ux-saved-instruction-source-defaults`

## Problem

The current beginner-UX work has become too scattered. Many small commits have
improved permission, sign-in, and recovery copy, but the work is no longer
driven by a single inventory, priority order, or completion contract. This plan
turns "treat every user as a beginner" into a tracked, evidence-first project.

Use the product contract and acceptance checklist in
[`docs/architecture/product-ux-direction.md`](../architecture/product-ux-direction.md)
as the standard. Do not invent a second UX standard in this plan.

## Goal

Every user-facing workflow should start with the shortest safe next action,
avoid internal implementation wording, and make recovery obvious for a
non-specialist operator.

This includes browser UI, Platform CLI flows, docs/runbooks, and API-facing
errors that can reach users.

## Non-goals

- Do not keep submitting unrelated one-off patches without linking them to this
  plan.
- Do not rewrite the app or introduce a new design system just to change copy.
- Do not hide useful diagnostics from troubleshooting views when they are already
  behind a deliberate "details" affordance.
- Do not treat legal policy text, code comments, test fixtures, protocol field
  names, or internal logs as product UX unless they are shown to operators.

## Current evidence

Recent commits on `ux-saved-instruction-source-defaults` already fixed several
role/access and recovery paths:

- `081218f3` sign-in role failures
- `05b53632` callback sign-in role failures
- `c89d6ac5` streamed chat role failures
- `62445693` agent control role failures
- `570240d8` sidebar role failures
- `cd67c572` settings role failures
- `ed805596` admin role failures
- `f5f8f4aa` role policy task summaries
- `051e6b60` clone retry role failures
- `5d9d02de` navigation role failures
- `911fb0d1` agent role failures

The repo already has useful gates:

- `npm run beginner:ux:copy`
- `npm run fsd:check`
- `npm run lint`
- `npm run format:check`
- `npm run typecheck`
- focused Vitest suites for many UI error-message helpers

Known candidate found while stopping ad hoc work:

- Saved-instruction create responses with `ok:false` and role/access details
  were confirmed to look like field validation instead of access guidance. Fixed
  under Phase 1 with coverage in `tests/unit/app/skills.store.test.ts`.

## Phase 0 inventory

This inventory is the work queue for the remaining phases. Status means:

- `ACTION` - needs implementation or a focused audit before this plan can close.
- `DONE` - current source and tests already cover the known beginner-UX risk.
- `WAIT` - no confirmed defect yet; review during the named phase before editing.

| Surface                                                      | Active files                                                                                                                                                                                           | Current coverage                                                                                                                                                                                                                                                                                                            | Status | Next action                                                                                                                                                     |
| ------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| First-run setup and `/start`                                 | `src/app/routes/start.tsx`, `src/app/routes/landing.ts`, `src/app/pages/getting-started/ui/GettingStartedView.tsx`, `src/app/layouts/sidebar/SidebarNav.tsx`, `src/app/shared/model/settings.store.ts` | `startRoute.test.ts`, `landing.test.ts`, `GettingStartedView.test.tsx`, `SidebarNav.test.tsx`, `AccountSection.test.tsx`, `settings.store-preferences.test.ts`, `i18nBeginnerErrors.test.ts`                                                                                                                                | DONE   | Phase 2 verified the skip/hide/reset contract. Continue only if a later audit finds a rendered first-run gap.                                                   |
| Sign-in and account recovery                                 | `src/app/routes/login.tsx`, `src/app/routes/public-auth.ts`, `src/app/features/auth/AuthPage.ts`, `src/app/shared/auth/AuthManager.ts`                                                                 | `AuthPage.test.ts`, `AuthManager.test.ts`, `routing.test.tsx`                                                                                                                                                                                                                                                               | DONE   | Keep regression coverage; do not reopen unless Phase 1 finds a new active raw auth error path.                                                                  |
| Agents list, detail, and lifecycle actions                   | `src/app/routes/agents.tsx`, `src/app/entities/agent/model/agents.store.ts`, `src/app/features/agents/*`, `src/app/widgets/agent-detail/AgentDetailView.tsx`                                           | `agents.store.test.ts`, `AgentListView.test.tsx`, `AgentDetailView.test.tsx`, `AgentControlPanel.test.tsx`, `CreateAgentModal.test.tsx`, `AgentPluginsTab.test.tsx`, `AgentTasksTab.test.tsx`                                                                                                                               | WAIT   | Phase 3 should review the full create/connect/start/restart/delete path as a user journey, not another role-error grep.                                         |
| Tasks board, list, timeline, and visual map                  | `src/app/routes/tasks.tsx`, `src/app/features/board/*`, `src/app/features/list/ListView.tsx`, `src/app/widgets/views/TimelineView.tsx`, `src/app/widgets/views/Workshop3DView.tsx`                     | `BoardView.test.tsx`, `TaskFormModal.test.tsx`, `TaskCard.test.tsx`, `ListView.test.tsx`, `Workshop3DView.test.tsx`, `BoardToolbar.test.tsx`                                                                                                                                                                                | WAIT   | Phase 3 should verify empty, loading, create, assign, blocked, failed, and retry states against the product UX checklist.                                       |
| Task detail, history, result review, and save-as-instruction | `src/app/features/detail/*`, `src/app/features/detail/model/*`, `src/app/widgets/agent-detail/AgentDetailView.tsx`                                                                                     | `TaskDetailPanel.test.tsx`, `HistoryTab.test.tsx`, `TaskMetadata.test.tsx`, `SkillDraftModal.test.tsx`, `reviewSnapshotErrorMessage.test.ts`, `taskDetailErrorMessages.test.ts`, `skillDraftErrorMessage.test.ts`                                                                                                           | WAIT   | Phase 3 should verify that result review and reusable-instruction next steps are clear after useful work completes.                                             |
| Chat                                                         | `src/app/features/chat/ChatView.tsx`, `src/app/features/chat/ChatComposer.tsx`, `src/app/features/chat/useChatStream.ts`, `src/app/shared/model/chat.store.ts`, `src/app/shared/model/chat.errors.ts`  | `ChatView.test.tsx`, `ChatComposer.test.tsx`, `useChatStream.test.ts`, `chat.store-errors.test.ts`                                                                                                                                                                                                                          | DONE   | Current stream and store errors have beginner-safe tests. Revisit only if Phase 3 finds a missing recovery state in the rendered chat flow.                     |
| Saved instructions                                           | `src/app/routes/skills.tsx`, `src/app/features/skills/*`, `src/app/features/skills/model/createSkillErrorMessage.ts`, `src/app/shared/model/skills.store.ts`                                           | `SkillsView.test.tsx`, `SkillCard.test.tsx`, `SkillDetailModal.test.tsx`, `SkillsToolbarStatus.test.tsx`, `createSkillErrorMessage.test.ts`, `skills.store.test.ts`                                                                                                                                                         | ACTION | Phase 1 should verify and fix the known `ok:false` create-response access case, then Phase 3 should review create/search/filter/empty states.                   |
| Settings: team/project/resources                             | `src/app/routes/settings.tsx`, `src/app/pages/settings/ui/*`, `src/app/pages/settings/model/workspaceSettingsErrorMessage.ts`, `src/app/features/settings/ResourcesSection.tsx`                        | `SettingsLayout.test.tsx`, `TeamsSection.test.tsx`, `WorkspaceRows.test.tsx`, `ResourcesSection.test.tsx`, `WorkspaceSettingsEmptyStates.test.tsx`, `workspaceSettingsErrorMessage.test.ts`, `AccountSection.test.tsx`                                                                                                      | WAIT   | Phase 2 verified the setup-checklist reset entry. Phase 3 should keep team/project destructive actions beginner-safe.                                           |
| Settings: runtime, providers, and code access                | `src/app/features/settings/RuntimeSection.tsx`, `ProvidersSection.tsx`, `GitCredentialsSection.tsx`, `SshKeysSection.tsx`, `KeysSection.tsx`, related error-message helpers                            | `RuntimeSection.test.tsx`, `ProvidersSection.test.tsx`, `GitCredentialsSection.test.tsx`, `SshKeysSection.test.tsx`, `KeysSection.test.tsx`, `runtimeErrorMessages.test.ts`, `providerSettingsErrorMessage.test.ts`, `gitCredentialsErrorMessage.test.ts`, `sshKeysErrorMessage.test.ts`, `platformKeyErrorMessage.test.ts` | ACTION | Phase 4 must cross-check local setup copy against the Linux/macOS/Windows CLI support guide. Phase 3 should verify setup prerequisites before advanced details. |
| Admin                                                        | `src/app/routes/admin.tsx`, `src/app/features/admin/*`, `src/app/shared/model/admin.store.ts`                                                                                                          | `AdminLayout.test.tsx`, `AgentsPanel.test.tsx`, `ControlPlanePanel.test.tsx`, `SystemHealth.test.tsx`, `admin.store.test.ts`, `adminErrorCopy.test.ts`                                                                                                                                                                      | DONE   | Keep current owner/admin route and recovery coverage. Revisit only if inventory finds a rendered raw admin error.                                               |
| Analytics                                                    | `src/app/routes/analytics.tsx`, `src/app/features/analytics/*`, `src/app/shared/model/analytics.store.ts`                                                                                              | `AnalyticsDashboard.test.tsx`, `ContextUsageDashboard.test.tsx`, `analytics.store.test.ts`, `StatCard.test.tsx`                                                                                                                                                                                                             | DONE   | Current empty and error states point to the data-producing action. Recheck during final audit.                                                                  |
| Billing                                                      | `src/app/routes/billing.tsx`, `src/app/features/billing/*`, `src/app/shared/model/billing.store.ts`                                                                                                    | `BillingBeginnerGuidance.test.tsx`, `BillingPage.test.tsx`, `BillingView.test.tsx`, `billingErrorMessage.test.ts`                                                                                                                                                                                                           | DONE   | Current billing guidance and access errors have focused tests. Recheck during final audit.                                                                      |
| Inbox and activity feed                                      | `src/app/routes/inbox.tsx`, `src/app/features/inbox/*`, `src/app/features/feed/*`, `src/app/shared/model/feed.store.ts`                                                                                | `InboxView.test.tsx`, `feed.store.test.ts`, `AgentStatusBar.test.tsx`                                                                                                                                                                                                                                                       | WAIT   | Phase 3 should verify that needs-action and empty states still point users to the next work item rather than only summarizing status.                           |
| Saved-item review and change history                         | `src/app/routes/context.tsx`, `src/app/routes/context-audit.tsx`, `src/app/features/context/*`, `src/app/features/governance/*`                                                                        | `ApprovalQueueView.test.tsx`, `AuditLogView.test.tsx`, `approvalQueueErrorMessages.test.ts`, `governanceAuditErrorMessages.test.ts`                                                                                                                                                                                         | DONE   | Current route loading, permission, network, and export guidance have focused tests. Recheck during final audit.                                                 |
| Platform CLI and operator docs                               | `rust/bins/cli`, `rust/crates/cli`, `docs/guides/cli-platform-support.md`, setup/deployment runbooks                                                                                                   | no single frontend test suite; validate with docs review and relevant CLI tests when implementation changes                                                                                                                                                                                                                 | ACTION | Phase 4 root CLI help now starts with setup commands. Continue the release/runbook cross-platform audit before closing Phase 4.                                 |
| Global UX gates                                              | `scripts/check-beginner-ux-copy.mjs`, `tests/unit/shared/check-beginner-ux-copy.test.ts`, `docs/architecture/product-ux-direction.md`                                                                  | `npm run beginner:ux:copy`, `check-beginner-ux-copy.test.ts`, `i18nBeginnerErrors.test.ts`                                                                                                                                                                                                                                  | DONE   | Keep this as a guardrail. Extend only when a repeated class of issue escapes focused tests.                                                                     |

## Plan

### Phase 0 - Inventory and triage

Deliverables:

- Build a surface inventory of active user-facing routes and controls:
  `/start`, Agents, Tasks, task details, Chat, Settings, Saved instructions,
  Admin, Analytics, Billing, Inbox, Governance, and setup/reset flows.
- For each surface, record:
  - primary beginner workflow
  - empty/loading/success/error states
  - current test coverage
  - highest-risk raw terms or missing next actions
- Classify each item as `ACTION`, `WAIT`, or `DONE`.

Acceptance:

- The issue checklist names the surface, file path, user-visible risk, and next
  action. **Done in the Phase 0 inventory above.**
- No code changes are made in this phase except documentation/checklist updates.
  **Done: this phase produced only this plan update.**

### Phase 1 - Error and recovery contract

Deliverables:

- Audit active error-message helpers and direct `catch` paths.
- Fix only confirmed active paths where the user sees:
  - raw status text like `HTTP 403`, `API 403`, `Code:`, `Details:`
  - internal concepts like roles, policy, backend, database, worker, parser, or
    stack traces
  - vague recovery text that does not name the next safe step
- Reuse existing helpers before adding new ones.

Acceptance:

- Each patch has one focused red test first.
- Each changed user-facing error includes a next action and avoids raw internal
  detail.
- Required checks pass: focused test, `npm run fsd:check`, `npm run lint`,
  `npm run format:check`, `npm run typecheck`, `git diff --check`, and the
  repository banned-reference scan.

Progress:

- Saved instructions: `ok:false` create responses with role/access details now
  reuse the saved-instruction access guidance instead of validation copy.
  Covered by `tests/unit/app/skills.store.test.ts`.

### Phase 2 - First-run and setup flow

Deliverables:

- Re-evaluate `/start` as a first-run surface, not a permanent navigation item.
- Decide and implement the intended behavior:
  - users who have completed setup can skip the tutorial
  - the Start navigation item can hide after setup is complete
  - Settings exposes a clear reset option
- Verify runtime/provider/CLI setup states explain prerequisites before advanced
  details.

Acceptance:

- A new user can see what to do first.
- A returning user is not forced back through the tutorial.
- Reset is discoverable from Settings.
- Tests cover first-run, completed setup, and reset behavior.

Progress:

- `/start` is no longer a permanent route for users who have hidden the setup
  checklist: `skipDismissedStartRoute` redirects to `/tasks` when
  `resolveLandingPath()` says the checklist is hidden.
- Landing is task-first by default. `resolveLandingPath()` returns `/start`
  only when the stored preference explicitly sets
  `gettingStartedDismissed: false`; missing, unreadable, or dismissed
  preferences open `/tasks`.
- The left navigation hides the Setup checklist item unless
  `shouldShowGettingStarted(preferences)` is true.
- Settings > Account exposes "Reset setup checklist" and makes the effect clear:
  it only adds the checklist back to the left menu, while projects, agents, and
  tasks stay unchanged.
- Focused coverage exists in `tests/unit/app/startRoute.test.ts`,
  `tests/unit/app/landing.test.ts`, `tests/unit/app/SidebarNav.test.tsx`,
  `tests/unit/app/GettingStartedView.test.tsx`, and
  `tests/unit/app/AccountSection.test.tsx`.

### Phase 3 - Core work surfaces

Deliverables:

- Review Agents, Tasks, task detail, Chat, Saved instructions, and Settings
  against the Feature UX Acceptance Checklist.
- Prioritize places where users make or recover from actions:
  create, save, start, retry, reconnect, delete, export, invite, revoke.
- Keep changes small and route them through the owning FSD slice.

Acceptance:

- Each action has visible prerequisites, a clear success state, and recoverable
  error copy.
- Empty states prefer a direct action over conceptual explanation.
- Frontend remains within strict FSD boundaries.

### Phase 4 - CLI and docs path

Deliverables:

- Audit Platform CLI and operator docs against
  [`docs/guides/cli-platform-support.md`](../guides/cli-platform-support.md).
- Ensure mainstream Linux, macOS, and Windows instructions are copy-pasteable
  where local CLI setup is supported.
- Move advanced details into troubleshooting or architecture sections.

Acceptance:

- Docs state prerequisites before commands.
- Commands use placeholders clearly.
- Success and next steps are explicit.

Progress:

- `docs/guides/cli-platform-support.md` already documents Linux, macOS, and
  Windows install paths with prerequisites, checksums, smoke tests, success
  states, and next commands.
- The Platform CLI root help no longer frames the tool as a developer/agent
  shortcut. It now tells first-time operators to connect to a Forge server,
  sign in, check agents, and enroll local Host CLI agents.
- `rust/crates/cli/src/cmd/root.rs` has a focused unit test covering the
  beginner setup commands in root help.
- Host CLI one-command join now prints reconnect commands that use the resolved
  `agentforge-sidecar` path instead of assuming it is on `PATH`, and the runbook
  gives both macOS/Linux and Windows PowerShell reconnect examples.
- `agentforge agents enroll-local --help` now states the sign-in prerequisite,
  local work folder expectation, success state, and beginner-safe option
  descriptions for project, tool, shell, and launch-block output.

### Phase 5 - Completion audit

Deliverables:

- Run a requirement-by-requirement audit against this plan and
  `product-ux-direction.md`.
- Confirm remaining issues are either fixed, moved to a new explicit issue, or
  documented as non-goals.

Acceptance:

- All planned checks pass.
- The issue checklist has evidence for every checked item.
- The active goal can only be marked complete after this audit proves completion.

## Working rules

- One coherent PR for this project; small fixes can be committed to the active
  branch but should still map to this plan.
- Do not open multiple PRs for small changes.
- Use subagents only for bounded inventory/review slices, not for interpreting
  this plan.
- Prefer evidence from active source, tests, and rendered behavior over broad
  grep counts.
- Stop on `WAIT` states instead of repeatedly polling CI or review status.

## Initial issue checklist

- [x] Phase 0: create the surface inventory and classify each item.
- [ ] Phase 1: audit and fix confirmed active error/recovery leaks.
- [x] Phase 2: settle `/start` skip/hide/reset behavior.
- [ ] Phase 3: review core work surfaces against the UX checklist.
- [ ] Phase 4: audit CLI and docs for beginner-safe, cross-platform setup.
- [ ] Phase 5: complete the evidence-based completion audit.

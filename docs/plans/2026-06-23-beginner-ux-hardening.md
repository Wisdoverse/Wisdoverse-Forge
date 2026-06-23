# Beginner UX hardening plan

Status: **Planning**
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

- Saved-instruction create responses with `ok:false` and role/access details can
  still be treated like field validation instead of access guidance. Verify and
  fix under Phase 1, not as an isolated patch.

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
  action.
- No code changes are made in this phase except documentation/checklist updates.

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

- [ ] Phase 0: create the surface inventory and classify each item.
- [ ] Phase 1: audit and fix confirmed active error/recovery leaks.
- [ ] Phase 2: settle `/start` skip/hide/reset behavior.
- [ ] Phase 3: review core work surfaces against the UX checklist.
- [ ] Phase 4: audit CLI and docs for beginner-safe, cross-platform setup.
- [ ] Phase 5: complete the evidence-based completion audit.

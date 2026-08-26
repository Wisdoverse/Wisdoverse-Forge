# Wisdoverse Forge — Long-Term Product Roadmap

| | |
| --- | --- |
| Status | Active |
| Owner | Product (with engineering) |
| Horizon | 18 months, phased P0–P4 |
| Review | Every phase exit; doc updated when a phase ships |
| Related | [Product UX Direction](docs/architecture/product-ux-direction.md), [SPEC.md](SPEC.md), [Runtime Validation](docs/runbooks/runtime-validation.md) |

This is the long-term plan for making Wisdoverse Forge a product a real team
can adopt, run, and trust — not an engineering artifact. It sets the standard
we hold each release to and records the exact work needed to get there. Phase
work lands incrementally; each phase ships only when its exit criteria pass.

---

## 1. Vision and Positioning

**Vision.** Wisdoverse Forge gives a team one governed workspace where requests
become managed AI work — assigned, watched, evidenced, reviewed, and reused —
without leaking control of data or the audit trail.

**Positioning.** Self-hosted, auditable AI workbench for teams that want the
productivity of agent teams with the governance of a well-run engineering
organization.

**What it is not (and will not become).**

- Not a hosted SaaS we operate for third parties.
- Not a general-purpose consumer chatbot.
- Not a framework or SDK — the battery is included.
- Not a place where agents act without a trace: every run keeps its evidence.

**Category definition.** "Governed AI work" = task + agent + execution + result
artifacts + evidence + reusable learning, all on one screen, all recoverable.

---

## 2. Personas and Jobs to Be Done

| Persona | Primary JTBD | Why they stop using a tool | Our success metric |
| --- | --- | --- | --- |
| **Operator / first-time host** (the person who runs the install) | Get the workspace to a working first agent task in one sitting | Setup takes more than one command or a step silently fails | Time-to-first-task ≤ 15 min; `make product` works untouched on a clean machine |
| **Team lead** (assigns work) | Send work, see who is doing what, review before trusting results | No way to see progress or what actually happened | Tasks reach "reviewable" state without paper-juggling; review is one click from the board |
| **Agent builder** (configures agents/skills) | Turn a working prompt/process into a reusable agent that does its job repeatedly | Skills cannot be found, attached, or measured | Skill reuse rate: % of completed tasks whose output got saved and later used |
| **Compliance / auditor** | Prove what was asked, by whom, with what access, and what came back | Evidence scattered in logs instead of a record | Every run links brief → context → artifacts → final review; audit export in one step |
| **Admin / SRE of the instance** | Keep the self-hosted stack healthy and know what to do when it is not | Errors that require reading source | Health page names the broken thing and the fix; SLO dashboard on by default |

---

## 3. North Star and Guardrail Metrics

**North Star.** *Weekly completed governed agent tasks per active workspace.*
Value only counts when the task round-trips: assigned → executed → evidenced →
reviewed → reusable learning captured.

**Activation.** *First-task-in-15-minutes rate.* New workspace reaches one
completed, reviewed task inside the first session.

**Guardrails (do not trade these for speed).**

- Crash-free sessions ≥ 99.5% (frontend); API 5xx rate < 0.5%.
- Task execution success: > 85% of started tasks reach a terminal state with
  results persisted; every start is explainable in the Updates tab.
- Evidence completeness: 100% of completed tasks have last-verdict + artifacts
  (or an explicit "no artifacts" record).
- Security: no privileged containers, no host PID, no docker-socket mounts by
  default (enforced by `platform/security.rs`); secrets never logged.

**Product-health signals (measured by the existing checks, extended over
phases).**

- `make beginner-audit` passes for the supported install path.
- `npm run metrics:contract`, `npm run protocol:contract` green.
- E2E suite covers the activation journey (sign-up → first reviewed task).

---

## 4. Product Principles

1. **Shortest safe path first.** The first screen of any surface states the
   next safe action; advanced knobs live one level down.
2. **Audited by default.** Everything that matters — dispatch, context, result,
   review — becomes a record without extra work.
3. **Self-host control.** Data stays on the team's machines; no phone-home
   analytics; provider keys encrypted with an operator-supplied key.
4. **Progressive capability.** New users never need to understand every
   governance primitive before assigning a task; depth unfolds as needed.
5. **Boring reliability.** One-command start, health page with next steps,
   recoverable errors, safe destructive actions.
6. **Reusable results.** Work worth repeating becomes a skill a team owns.

These are the values every acceptance check in
[Product UX Direction](docs/architecture/product-ux-direction.md) expresses.

---

## 5. Quality Bar (applies to every phase)

Product surfaces must pass the Feature UX Acceptance Checklist in
[Product UX Direction](docs/architecture/product-ux-direction.md):

- Shortest safe path / plain-language outcome / visible prerequisites / clear
  success state / recoverable errors / safe destructive actions / progressive
  detail / cross-platform CLI path / testable operator path.

Engineering gates per change (from `AGENTS.md`):

- Frontend: `npm run fsd:check`, `npm run lint`, `npm run format:check`,
  `npm run typecheck` + affected Vitest project.
- Rust: narrow test first, `cd rust && make ci` when shared crates, API
  contracts, orchestration, auth, DB, or platform security change.
- Runtime/deployment: run the Compose target and verify service health plus
  the orchestration chain.

User-visible quality targets:

- p75 first-load < 3.5s on a fiber-class connection for the app shell; routes
  load without layout shift; focus-visible everywhere; keyboard nav reaches
  every primary action; ARIA roles match data-testid contracts.
- No dead ends: every empty state has one direct action with a success
  condition; every error has a retry or setup path.

---

## 6. Phases

### P0 — Ready for first teams *(shipped)*

**Objective.** A stranger can install, start, sign up, and reach a reviewed
task in one session with zero source reading.

**Scope.**

- One-command start (`make product`): bootstrap → stack → health wait →
  browser app → open browser → stop everything cleanly on exit.
- First-run landing: new sign-ins land on the `/start` checklist; skip/complete
  persists so return visits go straight to the board.
- Product framing of docs: README switches from "engineering preview" to
  product + readiness framing; roadmap linked everywhere.
- Activation e2e: sign-up → first reviewed task covered in Playwright.
- Health/operations polish needed for trust (SRE-relevant admin surfaces).

**Success measures / exit criteria.**

- Clean-install `make product` → browser → first task completed and reviewed,
  in under 15 minutes, without opening docs beyond this one.
- `/start` is the post-sign-up destination for fresh workspaces.
- README states what the product is, its status honestly, and one 10-minute
  path to value.
- Activation e2e green in CI; `make beginner-audit` green for local path.

### P1 — Daily-workflow excellence *(in progress)*

*Shipped so far: human updates (comment / blocker / unblock) as first-class
task records behind `GET|POST /tasks/{id}/comments`, shown on the task detail
Updates tab, and surfaced on the board via `GET /tasks/comments/latest`
(red "Blocked by a person" badge with author/body tooltip); skill attach-back
(`PUT|DELETE /skills/{id}/agents/{agent_id}` plus inline attach/detach in the
skill draft published state and the skill detail view); per-skill usage counts
(`GET /skills/{id}/usage` — injections, distinct runs, last used — shown in
the skill detail view) and the agent-side followed-skills view
(`GET /agents/{id}/skills`, chips on the agent profile); board search / agent
/ priority filters (already present) and the global `n` shortcut for New Task.
Remaining in scope: board inline updates.*

**Objective.** A team uses the board every day: humans, agents, and evidence in
one coherent flow.

**Scope.**

- Human comments and blocker updates as first-class task records, separate
  from execution attempts and lifecycle state (Product UX "Remaining Product
  Gaps" #1).
- Save-as-skill attach-back: after publishing a skill draft, one clear next
  action attaches it to matching agents, with usage counts visible later.
- Task board filters (agent, project, status, priority), inline updates, and
  keyboard shortcuts to match the speed of the command palette.
- Skill library: search, version history, attach/detach, and "last used" per
  agent.
- Chat-agent experience: single clean conversation surface with context-aware
  handoff into tasks.

**Success measures / exit criteria.**

- A task can be commented and blocked without changing its execution data; the
  block shows on the board.
- > 50% of completed tasks with reviewable output expose a one-click "save as
  skill" and users can see where the skill is attached.
- Task list interactions (filter/search/reassign) covered by
  data-testid-anchored e2e tests without flake (lazy-route `waitFor visible`
  pattern per README).

### P2 — Teams, governance, and trust *(in progress)*

*Shipped so far: compliance task-history export (`GET /tasks/export` → CSV,
buttons on the board and the governance audit page; the export action itself is
audited) alongside the existing context-governance audit export; team/project
member management and invite routes already exist from the current surface.
Version notification shipped: Settings → About offers a self-host-friendly
manual "Check for updates" (latest GitHub release vs installed version, with
up-to-date / newer / unreachable states). Operations overview shipped
(`/operations`): runtime readiness, AI services, agent availability, queue
flow, and system health in one refreshable triage view with a direct next
action per item. Backup/restore verification drill executed on the local
stack (backup → wipe → restore: 1,000-task dataset restored exactly, 86
migrations intact) with evidence recorded in the disaster-recovery runbook.
Approval-gate polish shipped: the task form offers a "Wait for my approval
before the agent starts" option (submit preview states where the task waits),
and the blocked → Needs approval → Approve detail affordance completes the
loop. P2 scope complete.*

**Scope.**

- Members: invite flow, roles (Owner/Admin/Member/Viewer), per-project access,
  and change audit.
- Approval gates for destructive agent work (delete/overwrite/publish) with
  reviewer identity recorded.
- Context governance: review-by-default for sensitive scope reuse, leakage
  checks at dispatch, per-workspace context policy visible to the user.
- Audit export (CSV/JSON) of tasks, runs, context decisions, and access for
  compliance review.
- Backup/restore productized: one command to snapshot, one runbook to restore
  and verify.
- Single-host production polish: upgrade path (`make prod-pull`), version
  notifications in the UI, and a health page that names the fix.

**Success measures / exit criteria.**

- 100% of privileged or destructive operations require confirmation naming the
  affected resource.
- Audit export covers every record type the docs claim, verified by a test.
- Restore drill completes in one documented session on a fresh host.

### P3 — Scale, platform, and isolation *(in progress)*

*Shipped so far: measured 1k-task board performance (huge board baseline first
card ≈ 3.5 s, 15 665 DOM nodes at 1000 cards) and shipped per-column
progressive disclosure (60 cards per column + "Show all N in this group"),
cutting the initial board DOM to ≈ 2 000 nodes (−87%) with one click to expand
a column. The List view was already virtualized. Next: virtualization of
board columns when drag-and-drop constraints allow, p95 API < 300 ms on
reference hardware, multi-organization ops surfaces.*

**Objective.** Multiple teams/workspaces and heavier load without new
architecture decisions by the operator.

**Scope.**

- Multi-organization self-host: one instance serves several organizations with
  hard tenant isolation (DB layer already tenant-scoped; finish UI + ops
  surfaces).
- Orchestrator HA + Temporal worker scaling; queue depth and backpressure
  surfaced on the health page.
- Plugin/runtime compatibility registry; agent image auto-update policy with
  rollback (extends cli-image-auto-update).
- Performance: p95 API < 300 ms on reference hardware; boards render 1k+
  tasks with virtualized lists; 3D/timeline views stay opt-in and lazy.

**Success measures / exit criteria.**

- Two organizations on one instance never cross-allocate or leak, proven by
  tenant-boundary tests.
- Reference-load soak passes with no SLO breaches.
- Upgrade/rollback of agent images rehearsed on a clean host.

### P4 — Intelligent, self-improving workspace *(in progress)*

*Shipped so far: context-budget warning before publish — the context preview
now sums the estimated tokens of selected items against the agent's
`max_context_tokens` and shows the usage line plus an amber/red warning when
the agent's context would be mostly or fully consumed. Auto-extraction
acceptance measurement shipped: draft open/publish events are recorded
(`skill_draft_opened` / `skill_draft_published`) and the Skills page shows
"Suggested drafts accepted: X% (M saved from N suggested)". Review copilot
shipped: finished tasks get a per-person review checklist (`GET|PATCH
/api/v1/orchestration/tasks/{id}/review-checks/…`) with a 4-item evidence
review, optimistic toggling, and a per-user saved progress. Queued-time
prediction shipped: waiting tasks show "Starts in ~N min" (queued-time
prediction for task dispatch) computed from the org's median completed-task
duration and the task's real queue position (same agent, else shared pool),
with a "why" hint naming the position and how to change it; honest
"rough guess" wording when no history exists yet. Context safety loop shipped:
context-overflow failures are recognized (`context length exceeded`,
`prompt is too long`, …), the board card and task detail explain the cause
and the trim-and-retry action, warning/failure analytics events is recorded
(`context_budget_warning` from the preview, `context_overflow_failure` from a
failed card), and Analytics shows "Context safety": warnings shown, overflow
failures, and the % that still overflowed despite a warning. Enterprise SSO
shipped: a generic OpenID Connect provider (Casdoor, Keycloak, Authentik, Entra
ID) can be enabled through `AUTH_SSO__*` — the login page gets a "Single
sign-on" button, the flow runs through `/api/v1/auth/sso/oidc` (+callback) with
cookie-bound single-use state, and first sign-in provisions the account (no
password, own team space). Existing members sign in by email match. Automatic
context compaction shipped: when the context preview exceeds the budget it
offers a one-click "Trim to fit" — pinned items are protected,
least-recently-used items go first, and the selection never empties; the trim
is measured (`context_trim_applied`) and shown on the Analytics "Context
safety" line. OIDC member sync/role mapping shipped: when
`AUTH_SSO__ROLE_CLAIM` + `AUTH_SSO__ADMIN_GROUPS` are configured, each sign-in
maps the provider's groups onto the org role — a member in an admin group is
upgraded to `admin` in their default team space, owners are never touched, and
nothing is ever demoted (measured and documented; no-op otherwise). Group→org
provisioning shipped: `AUTH_SSO__ORG_GROUP_MAP` (`orgSlug=group;…`) adds users
to the mapped org when their provider groups match (member, or admin with an
admin group), and `AUTH_SSO__DEPROVISION` removes the membership when the group
leaves — owners and the user's last membership are always protected. Next: P4
exit-criteria review (round 24): a fresh-DB one-command start was verified
(volumes wiped; migrations 001–088 applied at boot; first account registered
and logged in; `/api/v1/orgs` healthy on the fresh schema). The real
orchestration E2E was revived against the current stack: the fixture seed
gained `runtime_kind` (migration 062 invariants) and the UI steps were
updated to the current form (New task button, dialog name, field labels, and
the project-first flow). The verified chain now covers browser → API →
orchestrator → NATS → real sidecar (durable assignment consumer bound) →
DB; the real-task E2E now passes end-to-end (round 25): the form automation was
completed (project-first flow, Task options disclosure, brief-confirmation
step) and the full chain browser → API → orchestrator → NATS → sidecar →
completed task → verified result runs green on the live stack. Exit criteria hooks
(skill acceptance, context-safety rate, prediction why+fix) are all shipped
and measurable in-app.

## P5 — Scale and governance for real teams *(proposed, pre-scoped)*

P4 made the single-workspace loop trustworthy. P5 targets the moment a team
of 10–50 runs it daily. Proposed investment areas, in priority order:

1. **Orchestration reliability at scale** — flaky-run visibility shipped (attempt
   notes on cards/guides + Properties attempt row) and queue batch retirement
   shipped: the board's "Retire stale tasks" (owner/admin, confirmed)
   batch-closes never-started backlog/queued tasks untouched for 7+ days via
   `POST /groups/{id}/tasks/retire-stale`, audited and capped (
   `olderThanDays` 1–90, `batchLimit` 1–500), and the confirm dialog now reports
   the live stale count (disabled "Queue is clean" state when nothing qualifies;
   stale = never-started backlog/queued untouched 7+ days). Next:
   retry-policy observability (backoff/limits).
2. **Identity & sync depth** — invited-member onboarding for non-SSO users
   shipped: a team lead can invite by email from Settings → Team members;
   people without an account get a one-time 72 h invite link
   (`team_invites` + `POST /invites/{token}/redeem`), redemption matches the
   invited email and grants org + team memberships (single-use, audited by
   email match). SSO group→team mapping shipped:
   `AUTH_SSO__TEAM_GROUP_MAP` (`teamName=group;…`, requires role_claim)
   grants/admin-only team memberships on sign-in and (with deprovision)
   removes them when the group leaves — unknown team names are skipped so a
   rename never blocks sign-in. Provider-side deprovisioning events shipped:
   `AUTH_SSO__DEPROVISION_TOKEN` enables `POST /api/v1/auth/deprovision` (provider
   webhook; constant-time header check) which immediately removes a user's
   non-owner memberships everywhere — revocation is instant instead of waiting
   for the next sign-in, and owners are never auto-removed. SCIM-style
   provisioning shipped: the same webhook token protects `POST /api/v1/auth/sso/provision`
   ({email, displayName, orgSlugs, roles}) which creates the account when missing
   and adds member/admin memberships for the requested org slugs (unknown slugs
   skipped). Full SCIM Users slice shipped: the same token protects
   `GET|POST /api/v1/auth/sso/scim/Users` and `GET|DELETE /Users/{id}`
   (CamelCase User/ListResponse/Error projections, `schemas`, `meta`,
   `active`; 1-based `startIndex`/`count` paging clamped 1..=100, default 50,
   `totalResults`, oldest-first order; `userName`-based creation with group
   slug → org membership mapping; DELETE strips non-owner memberships and
   deactivates the account — deactivated users leave the list and 404).
   Next: SCIM Groups resource + attribute extensions (name/emails).
3. **Team workflow depth** — task templates shipped: Settings → Task templates saves a
   reusable brief (name + title + brief + priority + approval flag) and the task form's
   "Saved by your team" grid applies it in one click (`GET|POST /api/v1/task-templates`,
   `DELETE /api/v1/task-templates/{id}`; org-scoped, validated, audited, delete limited
   to the creator or an owner/admin). Required acceptance gates shipped:
   `REVIEW_REQUIRED_GATES` (comma-separated known check keys, validated at boot)
   turns checklist items into gates — a human cannot mark a task completed until
   every required key is ticked by any reviewer (`GET /orchestration/tasks/{id}/review-gates`
   + enforcement on PATCH-complete and `POST /tasks/{id}/complete`), with "Required"
   tags and pending/all-clear hints in the checklist. Scheduled/recurring tasks
   shipped: Settings → Task templates → Recurring tasks creates a schedule (name,
   title, project, waiting place, cadence 15 min–30 days, approval flag) backed by
   `recurring_tasks` + a 60 s server runner (`POST|GET /api/v1/recurring-tasks`,
   `PATCH|DELETE /api/v1/recurring-tasks/{id}`); each due tick creates one
   unassigned task (next available agent picks it up), at-most-once claiming, and
   pause/resume/remove controls. Scheduled compliance exports shipped:
   `COMPLIANCE_EXPORT_INTERVAL_HOURS` + `COMPLIANCE_EXPORT_DIR` (pairing checked
   at boot) make the server write per-org CSV snapshots of the latest 1000
   tasks on a cadence, with `.last_run` restart-safety; each org gets
   `<dir>/<org-slug>/agentforge-compliance-<timestamp>.csv`. Telemetry
   retention shipped: `ANALYTICS_RETENTION_DAYS` (0 off; boot + 6 h sweep)
   purges `events`/`analytics_events` older than the window — task, run,
   comment and review records are never touched. Project-scoped task
   templates shipped: a template can be limited to one project (Settings
   picker + `projectId` on `POST /api/v1/task-templates`; `GET /task-templates?projectId=`
   returns that project's own plus team-wide templates, and the task form
   refetches when the project changes). Multi-step waits shipped: the task
   form's "Wait for these tasks first" (params `dependency_ids`; up to 5, ≤10)
   starts a task blocked on `waiting_dependency`; it never auto-dispatches
   until every prerequisite is `completed` (failed/canceled stays stuck for
   re-planning), and completing a task releases dependents whose prerequisites
   are then all done (best-effort, post-commit, re-dispatch). Next:
   artifact/run retention beside object storage.
4. **Governance reporting** — per-agent success, usage, and cost trends
   shipped: Analytics gains a "Work reliability" list
   (`GET /analytics/agent-reliability`, 1 h–1 y window, default 30 d) with
   finished runs, failures and tone-coded success rates from
   `orchestration_tasks`, plus an "Agent usage" list (`GET /analytics/agent-usage`,
   same windows) with assistant requests, input/output tokens, share of the
   window's total (from `agent_messages`), and `estimatedCost` per agent when
   the operator sets `LLM_PRICING` (USD per 1M tokens per model, validated at
   boot; missing models show no estimate). Next: scheduled compliance
   exports, retention policies.
5. **Operator hardening** — offline install bundles shipped:
   `scripts/offline-bundle.sh` (with `--full-stack`) packages Forge images
   plus the pinned platform services into one verified tar (`SHA256SUMS`,
   tag list, README); `scripts/load-offline-bundle.sh` verifies and loads it
   on the air-gapped host, and Ed25519 bundle signing (`BUNDLE_SIGNING_KEY` →
   `SHA256SUMS.sig`, verified with the public key; pkeyutl raw-in) provides a
   TUF-style chain-of-trust starter — see `docs/guides/offline-install.md`.
   Full TUF-style metadata shipped (item 9 below): root pinning + key rotation
   via the operator CLI. OpenTelemetry trace export shipped (item 8).
   Operator-facing consolidation also
   shipped: a one-page
   self-host runbook (`docs/runbooks/self-host-ops.md`) with every config knob,
   a weekly checklist, incident pointers, and rotation/upgrade steps.

8. **Observability & platform telemetry** — OpenTelemetry trace export
   shipped: `OTEL_EXPORTER_OTLP_ENDPOINT` (unset = zero-cost no-op) exports
   spans from the API server and orchestrator over OTLP gRPC (default) or
   `http/protobuf` (`OTEL_EXPORTER_OTLP_PROTOCOL`) with the standard SDK
   sampling knobs (`OTEL_TRACES_SAMPLER`, `OTEL_TRACES_SAMPLER_ARG`) and
   `OTEL_SERVICE_NAME` resource override; W3C `traceparent` contexts join the
   API → NATS → sidecar → container-CLI hops into one trace (see
   `docs/guides/configuration.md`). Prometheus `/metrics` stays the metrics
   path; Next: OTLP metrics + logs export.
9. **Supply-chain trust (offline bundles)** — full TUF-style metadata shipped:
   `agentforge tuf` (init/sign/verify/rotate) writes a root → targets →
   snapshot → timestamp chain under `metadata/`, signed with the bundle
   Ed25519 key; `verify` enforces root pinning (byte-identical same version,
   signed-by-pinned-key rotation, rollback rejection) and checks every payload
   file hash+size; `rotate` re-signs a new root with old+new keys (grace
   period). The bundle scripts call it automatically when the CLI is on PATH
   (see `docs/guides/offline-install.md`).

Each area keeps the P3/P4 guardrails: beginner-audit green, activation E2E,
evidence completeness, and no phone-home. P5 begins only after the P4 exit
criteria are re-checked on a clean install with the completed real-task E2E.*

**Scope.**

- Context budgeting: predict and warn before an agent's context window is
  overrun; automatic compaction suggestions.
- Skill auto-extraction: from a completed task, a drafted skill the human
  approves — measured by acceptance rate.
- Review copilot: checklist of verification steps for the reviewer of a
  task's artifacts.
- Work estimates and queued-time predictions for task dispatch.
- Enterprise SSO (Casdoor, OIDC) and member sync for larger orgs.

**Success measures / exit criteria.**

- Auto-drafted skills accepted ≥ 40% (humans review rather than ignore).
- Context warnings prevent ≥ 90% of context-overflow failures on reference
  tasks.
- Any prediction surface ships with a "why" explanation and a way to correct
  it.

---

## 7. Measurements and Cadence

- Weekly: `make beginner-audit` output kept green; PRs that degrade it are
  blocked until fixed.
- Per release: activation e2e, product-health signals, and the three guardrails
  reported in the release notes.
- Per quarter: revisit the roadmap; move an item only with measured evidence
  (or drop it).

## 8. Risks and Mitigations

| Risk | Mitigation |
| --- | --- |
| Agent container is a supply-chain/live-code surface | Trusted base images, two-layer image model, `platform/security.rs` deny-by-default, SBOM on image build |
| Vendor CLI licenses keep some agents build-on-your-own | Clear `make build-agent` documented path; updated list in CLI image docs |
| LLM/provider cost surprise in self-host | Per-workspace provider API keys, budget/limits surfaced in settings, cost recorded per run (analytics) |
| Single-org maturity vs multi-org ask | Ship P2 governance before P3 multi-org; tenant scoping already enforced at the repo layer |
| Running every service on one host | Keep the documented single-host path as the supported baseline; HA is explicit P3 scope with its own validation |

## 9. Explicitly Out of Scope (next 18 months)

- Operating a public hosted service.
- Mobile apps.
- Third-party plugin marketplace with payment rails.
- Cloud-managed runtime as the default execution path.

---

## 10. How This Plan Is Executed

Each phase above exits through its own validation, then the next phase starts.
Work is tracked as pull requests against this plan; the phase marker moves only
when the phase's exit criteria pass. When a phase changes how operators run
Forge, the matching runbooks are updated as part of the same change.

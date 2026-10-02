# Wisdoverse Forge Product Roadmap

| | |
| --- | --- |
| Status | Active engineering direction; product adoption remains optional to evaluate |
| Owner | Product with engineering; assign a delivery owner to each implementation PR |
| Near-term horizon | Deliver and validate the ordered maintenance workflow against technical acceptance evidence |
| Longer-term horizon | Demand-triggered investment; adoption evidence may inform priorities but does not gate current delivery |
| Related | [Product UX Direction](docs/architecture/product-ux-direction.md), [Product Validation Guide](docs/guides/product-validation.md), [SPEC.md](SPEC.md), [Runtime Validation](docs/runbooks/runtime-validation.md) |

## 1. Direction and Initial User

Forge remains a self-hosted, governed AI workbench for teams. The next cycle
focuses on recurring repository maintenance: turn an agreed maintenance request
into a reviewable change, keep execution and verification evidence, and make
failed work straightforward to recover or hand back to a person.

The first workflow to deliver is dependency upgrades and failed-PR repair for
small software teams maintaining repositories. Begin with one approved
repository. Multiple repositories are a later step, after a concrete routing
or access need justifies the added scope. Teams may optionally evaluate
adoption and human effort; recruiting teams, waiting four weeks, or measuring a
target reduction does not block implementation or merge.

This is a product hypothesis, not a claim that Forge already operates a
maintenance service or that this is its permanent exclusive scope. Existing
chat, project, task, and runtime capabilities remain supported.

General assistants, collaborative workspaces, and agent harnesses increasingly
provide shared context, scheduling, tools, approvals, and execution. Those
features alone do not establish a reason to adopt Forge. The hypothesis is that
teams with a reason to self-host can retain control of work records and runtime
access while spending less human time on a recurring maintenance workflow.
Compare that hypothesis against the team's actual tools.

### Responsibilities

- Forge owns task routing, durable work records, access policy, execution
  visibility, review evidence, and handoff.
- Supported Container CLIs perform the work behind the existing runtime
  boundary. Reuse their capabilities and evaluate adapters rather than
  duplicating every harness feature.
- GitHub remains the source of repository, PR, and CI facts. A team may review
  there; reducing duplicate administration is a product goal.
- Execution completion, verification, acceptance, merge, and deployment are
  different outcomes. Report them separately.
- Reusable skills are optional: save durable procedures when useful, rather
  than requiring every task to produce a skill.

## 2. Existing Foundation and Its Limits

These are implementation baselines, not pilot adoption results. Runtime claims
remain limited to the deployment, revision, and checks recorded in the linked
runbooks and tests.

| Foundation | Existing support | Boundary or outstanding evidence |
| --- | --- | --- |
| First-run path | `make product`, Start checklist, task board, activation E2E | Preserve clean-install verification and measure actual first-task time |
| Team work | Human comments and blockers, templates, recurring tasks, prerequisite waits, skills and usage | Board inline priority updates and stale-refresh recovery have local browser/API evidence; workflow usefulness may be evaluated separately |
| Review and evidence | Task runs, run-scoped evidence, review checklist and required checklist gates | A terminal run or a checked box does not establish that the change is correct |
| Self-fix PR delivery | Server-owned GitHub App, Maintenance repository settings with read-only preflight and default-branch discovery, draft-PR bridge, CI checks, expected-head merge, human approval | Deployment-level single-repository configuration; preflight reports prerequisites, not CI success; sensitive changes require review on GitHub |
| Governance and operations | Roles/invites, OIDC and provisioning, audit exports, health/update surfaces, backup/restore guidance | Validate the supported deployment before real team use; advanced identity needs remain demand-triggered |
| Runtime and supply chain | Multiple Container CLIs, isolation policy, signed offline bundles, TUF-style metadata, OpenTelemetry traces | Per-CLI runtime proof, compatibility and recovery evidence are required; OTLP metrics/logs remain future work |
| Analytics | Agent reliability, token usage/cost estimates, skill acceptance and context-safety signals | No recorded pilot baseline for human effort, accepted outcomes, or sustained use |

The [Self-Fix Loop](docs/guides/self-fix-loop.md) currently opens changes for the
configured repository and requires deliberate human approval to merge.
Automatic dispatch, deployment, and merge are outside that loop. The next
cycle must not silently expand those permissions.

The former P0–P5 roadmap established this foundation. P2's recorded scope was
implemented, but that does not prove team adoption. P3 performance/scale exit
criteria and P4 effectiveness targets must be evaluated separately from
whether their UI and measurement hooks exist. Do not treat the old phase labels
as evidence that every phase passed.

## 3. Direct Engineering Delivery

Engineering delivery proceeds against the ordered backlog below and the normal
review, CI, migration, and runtime gates. Product adoption is a separate,
optional evaluation. Do not wait for a pilot cohort, four weeks of repeat use,
or a measured 30% effort reduction before implementing or merging an item.
Technical gates require executable evidence through the real API and database,
Platform CLI where applicable, recovery behavior, and migration compatibility
on the supported runtime. Static inspection alone does not pass those gates.
Record the tested revision and environment in the linked runbook; local proof
does not establish production readiness.

The implementation order and current evidence are in
[Ordered Delivery Backlog](#4-ordered-delivery-backlog). Follow the
[Product Validation Guide](docs/guides/product-validation.md) when a team
chooses to measure product outcomes. Keep evaluation records on team-owned
systems; Forge does not require phone-home telemetry or publication of private
pilot details.

## 4. Ordered Delivery Backlog

This backlog includes implemented foundations and planned workflow additions.
Each implementation PR must distinguish existing primitives it reuses from
behavior it adds and provide the relevant operator or runtime proof.

1. **Repository setup.** Make the approved repository, default branch,
   connection permissions and verification prerequisites explicit. The read-only
   preflight, default-branch discovery and Maintenance repository settings with
   bounded checks and recovery guidance are implemented in
   [PR #1195](https://github.com/Wisdoverse/Wisdoverse-Forge/pull/1195). Local
   operator-path proof is recorded in [Runtime Validation](docs/runbooks/runtime-validation.md#maintenance-repository-settings-local-proof).
   Validate the selected repository and its branch protection as engineering
   prerequisites. Keep the supported setup to one repository;
   broader repository/tenant configuration needs an explicit design and proof.
2. **Source-to-result linkage.** Associate the source request or PR with its
   task, execution attempt, starting revision, produced change and current
   external state. Prevent duplicate work when the same source is submitted
   again. Start with deliberate submission; webhook/event intake is a separate
   capability requiring deduplication and permission validation. The deliberate
   intake API, transactional deduplication, tenant constraints, Tasks-page
   submission form and task-detail source/result view are implemented; see the
   [Self-Fix Loop guide](docs/guides/self-fix-loop.md#submit-and-trace-a-maintenance-source).
   Local browser workflow proof is recorded in
   [Runtime Validation](docs/runbooks/runtime-validation.md#maintenance-browser-workflow-local-proof).
   Real API/database and agent/GitHub paths, migration compatibility, and
   production operation require their own executable validation; local browser
   proof is not production acceptance. Webhook intake remains out of scope for
   this item.
3. **Verification reports.** Give reviewers the scope, changes, checks,
   failures, unverified areas and evidence for the exact revision. Separate
   operator-reported checks from a fresh GitHub observation and a human
   verdict. Revision-bound report submission, exact-revision GitHub observations,
   and human acceptance requiring a finished run and a fresh matching GitHub
   head are implemented in [PR #1196](https://github.com/Wisdoverse/Wisdoverse-Forge/pull/1196).
   Commands in a report are recorded, not executed. Validate report and verdict
   behavior through the real API/database path, including stale-revision and
   migration behavior. See the
   [Maintenance delivery guide](docs/guides/maintenance-delivery.md) and the
   [local delivery proof](docs/runbooks/runtime-validation.md#maintenance-delivery-local-proof).
4. **Recovery and handoff.** Expose retry limits/backoff, retain useful
   artifacts and failure causes, and let a person continue blocked work.
   Reconcile changed PR heads and CI state rather than relying on stale
   approval. Source implements bounded recovery snapshots and handoff records,
   including the task/revision/run context. Existing sensitive-path refusal and
   expected-head guards remain. See the
   [Maintenance delivery guide](docs/guides/maintenance-delivery.md) and the
   [local delivery proof](docs/runbooks/runtime-validation.md#maintenance-delivery-local-proof).
5. **Runtime comparison.** Exercise equivalent tasks through supported CLIs
   and produce common task/result/evidence records. The implementation records
   report conditions and identifies whether conditions match and
   distinct supported CLIs are represented. This is condition comparison of
   recorded evidence, not proof of real CLI execution or a benchmark. Real
   per-CLI runs and interrupted-execution/artifact-retention validation remain
   pending as executable runtime gates. Do not promise lossless vendor-session
   migration. See the
   [Maintenance delivery guide](docs/guides/maintenance-delivery.md) and
   [runtime validation](docs/runbooks/runtime-validation.md#maintenance-delivery-local-proof).
6. **Outcome measurement.** Source implements a submission cohort that retains
   awaiting-review, failed and canceled tasks, with the latest report and
   latest verdict represented once per task. Human effort uses six cumulative
   categories; unknown minutes remain unknown, not zero, and the dashboard
   shows the complete-minute denominator. Paired baselines are shown with
   their limits and do not establish savings by themselves. Pilot measurement,
   real-world effort outcomes, and skill effectiveness on later work remain
   to be evaluated. See the [Maintenance delivery guide](docs/guides/maintenance-delivery.md)
   and [runtime validation](docs/runbooks/runtime-validation.md#maintenance-delivery-local-proof).
7. **Board workflow friction.** Current changes implement inline task-priority
   updates and stale-refresh recovery with retry. Focused component tests and
   the full unit suite pass. Eight local browser-to-API scenarios also passed,
   covering persisted priority updates and retention/retry after an aborted
   refresh; see [current engineering validation](docs/runbooks/runtime-validation.md#current-engineering-validation-2026-10-02).
   Approval-queue and empty-state work remains demand-led and should address an
   observed workflow need.

## 5. Success Measures

**Optional product outcome measure:** weekly human-accepted maintenance changes
per active workspace, accompanied by total human effort and quality.
Execution success and task volume are operational signals, not substitutes
for accepted results.

### Optional product-evaluation targets

Teams may use these targets after establishing a baseline. They are optional
experiment targets, not implementation or merge gates, observed results, or
service guarantees.

- Recruit 3–5 pilot teams; seek at least 3 teams using the workflow in each of
  four consecutive evaluation weeks.
- Seek about 30% less human time per comparable task, including setup
  allocation, handling, review, recovery and rework.
- Start with 20–30 representative tasks to find failures and estimate effort.
  Report task mix and sample size; do not infer production-wide reliability
  from this small exploratory sample.
- Every reviewed result identifies the revision, evidence, unmet criteria
  and human verdict. Count unsuccessful and rejected work in the cohort.
- Quality must remain within the pilot's agreed acceptance criteria. Record
  post-acceptance regressions and reopen decisions.

Track wall-clock lead time, model/CI/infrastructure cost and self-host operation
time separately. A faster agent run is not evidence of reduced human effort.

### Activation and capability effectiveness

- Retain the first reviewed task within 15 minutes as an activation target,
  with prerequisite preparation reported separately.
- Existing skill-draft acceptance (previous target 40%) measures whether a
  draft is saved. Effectiveness requires subsequent task evidence.
- Existing context-warning prevention (previous target 90%) requires a
  defined reference set and comparison; warning counts alone cannot show
  prevention.
- Predictions require a visible explanation, uncertainty and a corrective
  action. Queue estimates remain estimates.

## 6. Quality and Permission Boundaries

- Preserve tenant scoping, isolated execution, credential protection,
  server-side sensitive-path refusal and human-approved self-fix merge.
- A green CI result is evidence about configured checks, not universal proof
  of correctness. A reviewer must assess the agreed task criteria.
- Preserve the operational targets: crash-free frontend sessions at least
  99.5%, API 5xx below 0.5%, and more than 85% of started tasks reaching a
  terminal state with persisted results. Report accepted outcomes separately.
- Completed work retains its verdict and artifacts, or an explicit no-artifact
  record. Interrupted work must expose its last known state and recovery path.
- Preserve one-command startup, beginner-audit, activation E2E, keyboard
  access, useful empty/error states and the runtime validation boundary.
- Keep metrics/protocol contract checks green. Retain the app-shell p75
  first-load target below 3.5 seconds on the documented reference connection;
  report the measurement environment rather than assuming this target is met.
- Apply the change-specific checks in [AGENTS.md](AGENTS.md) and
  [CONTRIBUTING.md](CONTRIBUTING.md). Documentation-only changes require
  `git diff --check`; UI, Rust, protocol and deployment changes require their
  respective checks and runtime proof.

## 7. Demand-Triggered Investment

Preserve existing capabilities while scheduling expansions when the workflow
or a pilot supplies a concrete need.

| Area | Trigger | Evidence before claiming readiness |
| --- | --- | --- |
| Multiple repositories and organizations | Repeat single-repository use and a demonstrated routing/access need | Repository permission isolation, external-state reconciliation, multi-tenant tests and operator path |
| HA and queue scaling | Measured capacity or availability bottleneck | Backpressure/recovery visibility, reference-load soak and upgrade/rollback rehearsal |
| Performance expansion | A measured workflow bottleneck | Reference hardware/data and p95 API/board measurements; the former p95 < 300 ms target needs a defined reference environment |
| Deeper identity integration | A pilot requires it | SCIM Groups/attribute compatibility and provisioning/deprovisioning proof |
| Artifact/run retention | Real storage growth or retention requirements | Documented policy, deletion boundaries and preservation of required audit evidence |
| OTLP metrics and logs | Existing traces/metrics do not resolve an operational need | Correlated runtime proof, redaction and export configuration guidance |
| Broader plugins and automation | A proven workflow needs a new tool or trigger | Adapter contract, least-privilege access, compatibility and retry evidence |

Mobile apps, a paid plugin marketplace, operating a public hosted service and a
cloud-managed runtime as the default are outside this cycle. Automatic merge or
deployment would require a separately reviewed product and permission policy;
they are not implied by this roadmap.

## 8. Evidence, Review and Documentation Maintenance

Use four distinct labels for roadmap items:

- **Planned:** a prioritized hypothesis or delivery requirement.
- **Implemented:** source and tests exist; cite the implementation PR.
- **Runtime-verified:** a recorded command or operator journey passed for a
  named revision and environment.
- **Adopted:** consenting teams demonstrate repeated use and measured outcomes.

Review workflow blockers and measurements weekly. At every stage exit, record
the evidence and decision. Change scope when the evidence calls for it; stop
expansion if teams do not repeat the workflow or human burden does not improve.

Keep this roadmap focused on priorities, boundaries and exit criteria.
Implementation steps and decisions belong in linked PRs/issues; delivered
changes belong in release notes/changelog; reproducible operating proof belongs
in runbooks. Follow [Documentation Rules](docs/README.md#documentation-rules)
and keep private pilot records out of the public repository.

Self-host control, cross-runtime support and governance are foundations to
evaluate, not an assumed competitive advantage. The next investment is earned
by demonstrated workflow value.

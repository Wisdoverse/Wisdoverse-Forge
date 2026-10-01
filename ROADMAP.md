# Wisdoverse Forge Product Roadmap

| | |
| --- | --- |
| Status | Active direction; maintenance workflow and adoption targets pending validation |
| Owner | Product with engineering; assign a delivery owner to each implementation PR |
| Near-term horizon | A 90-day validation cycle, starting when pilot owners and the baseline method are agreed |
| Longer-term horizon | Demand-triggered investment after the validation review |
| Related | [Product UX Direction](docs/architecture/product-ux-direction.md), [Product Validation Guide](docs/guides/product-validation.md), [SPEC.md](SPEC.md), [Runtime Validation](docs/runbooks/runtime-validation.md) |

## 1. Direction and Initial User

Forge remains a self-hosted, governed AI workbench for teams. The next cycle
focuses on recurring repository maintenance: turn an agreed maintenance request
into a reviewable change, keep execution and verification evidence, and make
failed work straightforward to recover or hand back to a person.

The first workflow to validate is dependency upgrades and failed-PR repair for
small software teams maintaining repositories. Begin with one approved
repository. Multiple repositories are a later step, after the first workflow
shows repeat use and lower human effort.

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
| Team work | Human comments and blockers, templates, recurring tasks, prerequisite waits, skills and usage | Board inline updates remain; workflow usefulness needs pilot evidence |
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

## 3. The Next 90 Days

Week numbers are planning windows, not release promises. A stage advances only
when its evidence is available; record delays and blocked criteria instead of
declaring success from elapsed time.

| Window | Outcome | Priority work | Exit evidence |
| --- | --- | --- | --- |
| Weeks 1–2 | A defined user, workflow and comparison baseline | Recruit 3–5 pilot teams; appoint pilot and delivery owners; agree acceptance criteria; collect representative upgrade/failed-PR tasks; measure the existing process | Consenting pilot owners, reproducible task samples, baseline method and an agreed review record |
| Weeks 3–6 | One repository produces reviewable maintenance changes | Productize repository setup, default-branch handling and prerequisites; add maintenance briefs; link source work to task/run/PR; present verification and handoff clearly | A documented single-repository path exercised on real tasks, with evidence tied to the exact change and human acceptance recorded |
| Weeks 7–10 | Repeat work and failures need less supervision | Address observed CI, conflict and external-state failures; make bounded retries and handoff visible; rehearse interrupted work; compare at least two supported Container CLIs where practical | Failure/recovery examples, no duplicate side effects in rehearsed retries, preserved artifacts, and comparable review evidence across tested CLIs |
| Weeks 11–12 | An explicit continue, adjust, or stop decision | Compare effort, quality, cost and four-week repeat use; document limitations; decide whether multiple repositories justify the next investment | Pilot readout with denominators, failures and uncertainty; a decision and the next prioritized scope |

Follow the [Product Validation Guide](docs/guides/product-validation.md).
Evaluation records stay on the team's systems; Forge does not require
phone-home telemetry or publishing private pilot details.

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
   Validate each pilot's repository and branch protection. Start with one
   repository per supported pilot setup;
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
   Real operator and pilot use, production migration, and real agent/GitHub run
   acceptance remain pending. Webhook intake remains out of scope for this item.
3. **Verification reports.** Give reviewers the scope, changes, checks,
   failures, unverified areas and evidence for the exact revision. Separate
   agent-reported completion from observed checks and human acceptance.
4. **Recovery and handoff.** Expose retry limits/backoff, retain useful
   artifacts and failure causes, and let a person continue blocked work.
   Reconcile changed PR heads and CI state rather than relying on stale
   approval. Existing sensitive-path refusal and expected-head guards remain.
5. **Runtime comparison.** Exercise equivalent tasks through supported CLIs
   and produce common task/result/evidence records. Test interrupted execution
   and artifact retention; do not promise lossless vendor-session migration.
6. **Outcome measurement.** Add accepted/rejected/rework outcomes and human
   setup, handling, review and recovery time beside existing usage metrics.
   Evaluate skill effectiveness on later work, not only draft acceptance.
7. **Observed workflow friction.** Prioritize board inline updates, approval
   queue and empty-state improvements only when they remove a measured step or
   a demonstrated pilot blocker.

## 5. Success Measures

**Primary product outcome:** weekly human-accepted maintenance changes per
active pilot workspace, accompanied by total human effort and quality.
Execution success and task volume are operational signals, not substitutes
for accepted results.

### Proposed pilot decision thresholds

Agree these targets with pilot owners after establishing the baseline. They
are experiment targets, not observed results or service guarantees.

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

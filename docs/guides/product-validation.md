# Product Validation Guide

Use this guide to evaluate whether Forge reduces the human effort of recurring
repository maintenance. The initial scope is dependency upgrades and failed-PR
repair on one approved repository. The targets in [ROADMAP.md](../../ROADMAP.md)
are hypotheses to test, not measured results.

## Prerequisites and Current Boundary

- A pilot owner, a reviewer and permission to work on the selected repository.
- A working self-hosted instance following [Getting Started](getting-started.md)
  and the relevant [Runtime Validation](../runbooks/runtime-validation.md).
- A configured provider or supported Container CLI, with recorded versions and
  execution access appropriate for the selected task.
- A task acceptance brief and the repository's normal verification procedure.
- For in-platform self-fix PR delivery, the GitHub App configuration described
  in [Self-Fix Loop](self-fix-loop.md), a checked maintenance repository, and a
  configured agent with access to the project files.

The implemented maintenance workflow supports deliberate request or pull
request intake into a selected project and task destination, a source/run/PR
trace, revision-bound verification reports, human verdicts with six cumulative
minute categories, handoffs, and an Analytics submission cohort with recorded
condition comparison for Container CLI reports. Use the [Maintenance delivery
guide](maintenance-delivery.md) for the operator path and interpretation of
these records. The repository connection is deployment-configured and its
default branch is discovered from GitHub. This workflow does not provide
webhook-driven automatic intake or per-task/multiple repository configuration.
Real runs with two different CLIs, production operation and pilot/adoption
evidence still require separate validation; implementation does not establish
that any team has adopted the workflow.

## First Evaluation

1. **Agree one representative task.** Write the requested change, starting
   revision, allowed scope, required checks and acceptance criteria. Identify
   what requires a person's decision before the task starts.
2. **Record the existing process.** Use the team's normal tools on a comparable
   task and keep its human time, checks, outcome and cost in the team's
   evaluation record.
3. **Run the Forge path.** For maintenance work, submit a source from **Tasks**
   after selecting a project and **Place for new tasks**. Review and assign the
   resulting task through ordinary task controls, then let the run finish.
   Follow [Maintenance delivery](maintenance-delivery.md) to record a
   revision-bound verification report and human verdict. For other work, use
   the [Task Workflow Guide](task-workflow.md).
4. **Review the exact change.** Inspect the diff and its GitHub observation for
   the reported revision. Keep operator-reported checks, provider-observed
   checks and the human verdict separate. Acceptance records a person's
   judgement; it does not merge the change.
5. **Record recovery and follow-up.** Use the handoff form for the blocker and
   next action when another person must continue. Record failed attempts,
   manual work, cost and later regressions in the team's evaluation record.

Success for this first evaluation means a reviewer can trace the request to
the run, change and verification evidence and make an explicit decision. It
does not require a merge, deployment or extracted skill.

## Comparable Tasks and Baseline

Start with 20–30 tasks across 3–5 consenting pilot teams. This is an exploratory
sample for discovering failure modes and effort, not a production reliability
benchmark. Include hard, unsuccessful and rejected tasks; do not select only
completed runs.

For each comparison, keep the repository starting revision, acceptance brief,
required checks and access scope equivalent. Run repeated experiments in
separate branches/work areas to avoid carrying changes from one treatment to
the next. Do not repeat production writes just to create a comparison.

Record model, Container CLI, instructions/skills, access and infrastructure
differences. Where possible use equivalent execution tools for both paths so
the comparison measures Forge's workflow rather than a model upgrade. Record
the order of trials and prior familiarity; reuse of a solved task can bias the
second run. Use historical comparable tasks or a shadow evaluation if a safe
paired trial is impractical, and state that limitation.

## Evaluation Record

Use the platform's maintenance report and human-verdict form for the recorded
checks, revision evidence, decision reason, and six effort categories. Keep the
remaining evaluation record private per task; cost and actual-pilot follow-up
are still team-owned records. Do not publish source code, credentials, internal
URLs or identifying team data in this public repo.

| Field | What to record |
| --- | --- |
| Task and scope | Evaluation ID, maintenance type, acceptance brief, allowed actions and source issue/PR if any |
| Starting state | Repository revision, baseline condition and pre-existing check failures |
| Execution setup | Forge revision/profile, CLI/model versions, instructions, skills and access scope |
| Attempts | Task/run IDs, retries, interruptions, failure causes and manual takeover |
| Result | Artifact or PR link, exact revision, diff and execution outcome |
| Verification | Check commands/results, environment, revision verified and unverified areas |
| Human verdict | Platform-recorded accepted/rejected/rework/reopened decision and reason; keep reviewer context and follow-up in the team record |
| Effort | Platform-recorded cumulative setup, handling, review, recovery, rework and operation minutes; unknown categories remain blank |
| Lead time and cost | Request-to-review/acceptance time, token/model cost, CI and infrastructure cost where available |
| Follow-up | Reopened work, post-acceptance regressions and later use of a saved procedure |

The maintenance outcomes dashboard supplies the submission cohort, latest
report and latest verdict per task, effort summaries, and report comparisons.
Use the private evaluation record for actual pilot activity, reviewer
coordination, costs, and follow-up that the platform does not measure. Mark
unavailable cost or timing as unknown, rather than zero.

## Metric Definitions

### Accepted work and quality

- **Weekly accepted changes:** distinct task outcomes accepted by a human in
  that week; retries are not additional accepted changes. Count merge and
  deployment separately.
- **Acceptance rate:** accepted tasks divided by all tasks in the defined
  submission cohort. The dashboard uses each task's latest report and latest
  verdict; submissions awaiting review and failed or canceled tasks remain in
  the cohort. Report those counts beside the rate; do not remove them after the
  experiment begins. Also show a decision-only rate if useful, with its
  different denominator explicit.
- **Rework:** human or agent work required after a review rejects or returns
  the result. Include it in effort even if the task is eventually accepted.
- **Regressions:** accepted results subsequently reopened or associated with
  an observed defect during the agreed follow-up window. Use a fixed window
  per cohort and report incomplete follow-up.

A terminal run, green configured checks or a checked review item is not a
substitute for the acceptance brief. Preserve evidence for the exact revision;
reassess when the change moves after review.

### Human effort, time and cost

Total human effort includes setup allocation, handling, review, recovery,
rework and an agreed allocation of instance operation time. Define shared
allocations before comparing tasks, and report one-time setup and ongoing
operation totals separately so the amortization is visible.

The dashboard counts a task in complete human effort only when its latest
verdict belongs to the current report and all six cumulative minute categories
are present. It reports that denominator explicitly; unknown categories are
not treated as zero. A baseline pair also requires a current report and
complete minute totals. For paired tasks, effort reduction is
`(baseline human minutes - Forge human minutes) / baseline human minutes`.
Report absolute minutes, task mix and per-task distribution alongside the
aggregate. If baseline effort is zero or unavailable, percentage reduction is
undefined. Baselines are human-reported estimates and the aggregate alone
cannot establish time saved. Elapsed agent runtime is not human time; report
lead time separately.

Report model, CI and infrastructure cost where known. Include failures and
retries in cost per accepted result; when no result is accepted, report total
spend and zero accepted results rather than a cost-per-result estimate.

### Repeat use and reusable procedures

A team demonstrates four-week repeat use when it submits real maintenance work
in each of four consecutive evaluation weeks. Also report weekly accepted
results, volume, inactivity and reasons for leaving; opening the app alone is
not workflow use. Count retained teams against all teams that started the pilot.

Saving a skill measures draft acceptance. Evidence that it helps requires
later comparable tasks, review outcomes and effort. Do not require skill
extraction from tasks that contain no useful reusable procedure.

## Failure and Recovery Rehearsals

Use a disposable task/work area and record the exact environment before
rehearsing CI failure, changed PR head, conflict, tool/provider unavailability
or execution interruption. Respect the deployed permission policy.

For each rehearsal, record the last persisted state, retained artifacts,
permitted next action, retry limit and eventual outcome. Check for duplicate
PRs or writes after retry. If continuation cannot be demonstrated, mark it
unsupported or requiring manual recovery; a durable task row alone is not
proof that a vendor session can resume.

## Pilot Review and Decision

Review blockers weekly and hold a decision review at the end of the roadmap
cycle. Start with the proposed targets: at least three teams using the workflow
for four consecutive weeks, about 30% less comparable human effort, explicit
review evidence and acceptable quality. Agree revisions to these targets after
baseline measurement and before evaluating success.

- **Continue:** repeat use and effort/quality evidence justify the next scope.
- **Adjust:** there is recurring demand, but a specific failure or workflow
  burden needs correction before expansion.
- **Stop expansion:** repeat use or benefit is absent; reconsider the initial
  workflow and installation burden before adding more platform features.

Record cohort sizes, uncertain measurements, failures and reasons for the
decision. Share only consented, anonymized aggregates or synthetic examples
publicly. Evaluation uses team-owned records; no phone-home analytics are
required.

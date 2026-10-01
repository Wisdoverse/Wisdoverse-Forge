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
  in [Self-Fix Loop](self-fix-loop.md), including production encryption setup
  and a configured container agent with access to the project files.

The current self-fix path uses a deployment-level repository configuration and
discovers its default branch from GitHub. A platform admin can check that
repository's access, starting revision and verification prerequisites using
`GET /api/v1/self-fix/repository`. The agent proposes a change; the server
opens a draft PR; a person reviews and explicitly approves merge. Sensitive
changes must be reviewed and merged on GitHub. Keep the pilot within these
supported boundaries until a separately validated change expands them.

General repository setup, source-PR import, automated intake and richer
verification reports are roadmap work. This guide does not imply those features
already exist. Use existing tasks and private manual evaluation records where
the current UI has no measurement field. It does not authorize automatic
dispatch, merge or deployment.

## First Evaluation

1. **Agree one representative task.** Write the requested change, starting
   revision, allowed scope, required checks and acceptance criteria. Identify
   what requires a person's decision before the task starts.
2. **Record the existing process.** Use the team's normal tools on a comparable
   task and record human handling/review time, checks, outcome and cost.
3. **Run the Forge path.** Create and assign a task using the existing
   [Task Workflow Guide](task-workflow.md). For a self-fix task, follow the
   existing PR workflow; do not infer permissions from this evaluation guide.
4. **Review the exact change.** Inspect the diff and checks for its revision.
   Record accepted, rejected or rework-required against the original brief.
   Keep execution status and the human verdict separate.
5. **Record recovery and follow-up.** Include failed attempts, manual work and
   later regressions. If the task is blocked, preserve the evidence and use the
   documented retry, setup or maintainer handoff path.

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

Keep a private record per task with the following fields. Do not publish source
code, credentials, internal URLs or identifying team data in this public repo.

| Field | What to record |
| --- | --- |
| Task and scope | Evaluation ID, maintenance type, acceptance brief, allowed actions and source issue/PR if any |
| Starting state | Repository revision, baseline condition and pre-existing check failures |
| Execution setup | Forge revision/profile, CLI/model versions, instructions, skills and access scope |
| Attempts | Task/run IDs, retries, interruptions, failure causes and manual takeover |
| Result | Artifact or PR link, exact revision, diff and execution outcome |
| Verification | Check commands/results, environment, revision verified and unverified areas |
| Human verdict | Reviewer, accepted/rejected/rework-required/awaiting-review, reasons and decision time |
| Effort | Setup, handling, review, recovery, rework and self-host operation minutes |
| Lead time and cost | Request-to-review/acceptance time, token/model cost, CI and infrastructure cost where available |
| Follow-up | Reopened work, post-acceptance regressions and later use of a saved procedure |

Existing analytics can supply usage and operational signals. Human effort and
acceptance fields may need manual recording until product measurement is
implemented. Mark unavailable cost or timing as unknown, rather than zero.

## Metric Definitions

### Accepted work and quality

- **Weekly accepted changes:** distinct task outcomes accepted by a human in
  that week; retries are not additional accepted changes. Count merge and
  deployment separately.
- **Acceptance rate:** accepted tasks divided by all tasks in the defined
  cohort. Report awaiting review, rejected, canceled and failed counts beside
  the rate; do not remove them after the experiment begins. Also show the
  decision-only rate if useful, with its different denominator explicit.
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

For comparable task cohorts, effort reduction is
`(baseline human minutes - Forge human minutes) / baseline human minutes`.
Report absolute minutes, task mix and per-task distribution alongside the
aggregate. If baseline effort is zero or unavailable, percentage reduction is
undefined. Elapsed agent runtime is not human time; report lead time separately.

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

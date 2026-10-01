# Maintenance delivery, review, and outcomes

This guide covers the operator record kept alongside a maintenance task: reported verification, the latest GitHub observation, a human verdict, and a handoff. These are separate records with different meanings. A saved report documents what an operator says was checked; it does not execute commands or turn a GitHub observation into a human verdict.

## First operator journey

1. **Check the repository prerequisites.** As a Forge platform administrator, open **Settings → Maintenance repository** and choose **Check connection**. Resolve any reported access or repository issue before starting work. This connection check verifies the configured repository and its preparation/review prerequisites; the task’s pull request still needs its own checks and human review. See the [self-fix loop guide](self-fix-loop.md) for repository setup details.
2. **Create the maintenance task.** Open **Tasks**, select the project and **Place for new tasks**, then expand **Maintenance request**. Submit either a stable request reference or a pull request number, with a title and brief. The request is saved as an unassigned backlog task. If a submission response is uncertain, retry the unchanged request with the same source reference; a repeated source opens the original task and keeps its original brief and destination. Choose **Open maintenance task** to continue.
3. **Assign and finish a run.** Review the task brief and assign an agent through the ordinary task controls. Wait for the run to finish and inspect the produced change. Refresh the task’s maintenance evidence to see its recorded revision, recovery state, and current GitHub observation.
4. **Save a verification report.** In the task’s **Verification, recovery and human review** panel, expand **Create verification report**. Record the scope, criteria, environment, change summary, any unverified areas, and checks with their commands, results, and evidence. A command entered here is a record only; the form does not run it. The report captures the finished run and revision metadata available at save time. Reported check results remain distinct from observed GitHub checks, their revision, and observation time.
5. **Record a human verdict.** Review the exact report revision and criteria, then record accepted, rejected, rework, or reopened with a reason. Acceptance is enabled only for a produced revision from a finished run with a fresh GitHub observation matching that revision. An accepted verdict records human judgement; it does not merge the pull request. The repository’s merge checks, approval controls, and expected-head protections remain independent gates.
6. **Record a handoff when another person must continue.** Expand **Record handoff for a person to continue**, add the blocker and next action, and save. This stores the current task, revision, run, recovery, and report context. Use task controls separately to stop, retry, or assign work; saving a handoff performs none of those actions. Active or queued work must be stopped and evidence refreshed before the handoff can be saved.
7. **Review the cohort in Analytics.** Open **Analytics** and inspect **Maintenance outcomes and human effort** for the selected project or team space and period. The cohort groups task submissions in the selected time window; the dashboard also shows review activity during the period, including tasks submitted earlier. Select reports from equivalent tasks to compare their recorded conditions, runtime, CLI, verdict, and effort.

## Interpreting records and totals

The report’s **reported checks** are operator-entered claims and evidence. **Observed GitHub checks** are a separate provider snapshot tied to a revision and check time. The human verdict is a third record. A GitHub observation does not establish that a reported command ran, and a reported pass does not establish that GitHub checks passed.

Record the six human-minute categories as cumulative totals for the task across attempts: setup, handling, review, recovery, rework, and operation. Leave a category blank when it is unknown; enter `0` only when no time was spent. Each new verdict replaces the previous verdict’s minute totals for the task in outcome summaries, so the latest verdict is counted once. The optional baseline is a separately reported estimate for equivalent human work. A baseline comparison alone cannot establish time saved.

The optional Container CLI comparison uses a shared comparison reference and checks whether reports describe matching task conditions and distinct supported CLI tools. A conditions match is not a benchmark result and does not establish comparative quality or performance; review the reports and evidence before drawing conclusions.

## HTTP API and retry contract

All delivery endpoints require authenticated, live platform-administrator access and use the caller's organization. A task from another organization is not accessible. The API returns the standard `{ "ok": true, "data": ... }` envelope; the delivery read returns `null` for an ordinary task without a maintenance source.

| Method and path under `/api/v1` | Purpose |
| --- | --- |
| `GET /self-fix/tasks/{id}/delivery` | Current revision, checks, recovery and bounded recent report/verdict/handoff history |
| `POST /self-fix/tasks/{id}/reports` | Save a verification input and captured revision/run snapshot |
| `POST /self-fix/tasks/{id}/decisions` | Record a human verdict on the latest report for the current run/revision |
| `POST /self-fix/tasks/{id}/handoffs` | Save a blocker, next action and retained context for inactive work |
| `GET /self-fix/reports/{id}` | Retrieve a retained report and its latest human decision |
| `GET /self-fix/outcomes` | Query a submission cohort with `from`, `to`, optional `projectId` and detail-page `cursor` |
| `GET /self-fix/comparison?reportIds={comma-separated IDs}` | Compare 2–8 distinct reports and show conditions that do or do not match |

Writes include a UUID `requestKey`, `expectedVersion` and `expectedRevision`; reports also include the current `runId` (or `null` when no run exists). Retry an unconfirmed write with the same key and unchanged input. An acknowledged replay returns the original record even if the task has since changed. Reusing a key for different input, or submitting stale version/run/revision state, returns a conflict; refresh evidence before preparing a new record. A report without a produced change must explain the missing artifact. The full input and response contracts are in [maintenance-delivery.ts](../../shared/types/maintenance-delivery.ts).

Outcomes default to seven days and accept windows up to 120 days. Summary denominators cover the entire selected submission cohort; the 100-row detail cursor does not reduce them. Review activity during the period counts distinct tasks, including older submissions. Read and write requests in the browser expire after 30 seconds and are discarded when the account, organization or task changes.

## Finding older reports and retention

The task panel shows a bounded recent history. To retrieve an individual report and its latest human decision, an authenticated platform administrator can use `GET /api/v1/self-fix/reports/{id}` with the report ID.

If a run or agent is explicitly removed, its report metadata and captured snapshot are retained; removing the live run link does not erase the recorded report. Raw artifacts referenced by a report remain subject to the existing artifact-storage retention policy. Deleting the task or organization cascades to its maintenance reports, decisions, and handoffs.

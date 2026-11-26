# Maintenance delivery, review, and outcomes

This guide covers the operator record kept alongside a maintenance task: reported verification, the latest GitHub observation, a human verdict, and a handoff. These are separate records with different meanings. A saved report documents what an operator says was checked; it does not execute commands or turn a GitHub observation into a human verdict.

## First operator journey

1. **Check the repository prerequisites.** As a Forge platform administrator, open **Settings → Maintenance repository** and choose **Check connection**. Resolve any reported access or repository issue before starting work. This connection check verifies the configured repository and its preparation/review prerequisites; the task’s pull request still needs its own checks and human review. See the [self-fix loop guide](self-fix-loop.md) for repository setup details.
2. **Create the maintenance task.** Open **Tasks**, select the project and **Place for new tasks**, then expand **Maintenance request**. Submit either a stable request reference or a pull request number, with a title and brief. The request is saved as an unassigned backlog task. If a submission response is uncertain, retry the unchanged request with the same source reference; a repeated source opens the original task and keeps its original brief and destination. Choose **Open maintenance task** to continue.
3. **Assign and finish a run.** Review the task brief and assign an agent through the ordinary task controls. Wait for the run to finish and inspect the produced change. Refresh the task’s maintenance evidence to see its recorded revision, recovery state, and current GitHub observation.
4. **Save a verification report.** In the task’s **Verification, recovery and human review** panel, expand **Create verification report**. Record the scope, criteria, environment, change summary, any unverified areas, and checks with their commands, results, and evidence. A command entered here is a record only; the form does not run it. The report captures the finished run and revision metadata available at save time. Reported check results remain distinct from observed GitHub checks, their revision, and observation time.
5. **Record a human verdict.** Review the exact report revision and criteria, then record accepted, rejected, rework, or reopened with a reason. Acceptance is enabled only for a produced revision from a finished run with a fresh GitHub observation matching that revision. An accepted verdict records human judgement; it does not merge the pull request. The repository’s merge checks, approval controls, and expected-head protections remain independent gates. A missing verdict does not block a Forge merge. A rejected verdict does: see [When Forge refuses to merge a rejected revision](#when-forge-refuses-to-merge-a-rejected-revision).
6. **Record a handoff when another person must continue.** Expand **Record handoff for a person to continue**, add the blocker and next action, and save. This stores the current task, revision, run, recovery, and report context. Use task controls separately to stop, retry, or assign work; saving a handoff performs none of those actions. Active or queued work must be stopped and evidence refreshed before the handoff can be saved.
7. **Review the cohort in Analytics.** Open **Analytics** and inspect **Maintenance outcomes and human effort** for the selected project or team space and period. The cohort groups task submissions in the selected time window; the dashboard also shows review activity during the period, including tasks submitted earlier. Select reports from equivalent tasks to compare their recorded conditions, runtime, CLI, verdict, and effort.

## When Forge refuses to merge a rejected revision

Forge refuses to merge a maintenance task when the latest verdict on its current report is **rejected**. A report is current while its revision, latest run, title, and brief still match the task. The **Finish this fix** button in the task's review panel is then disabled and explains that a reviewer rejected this version. If a merge request reaches the server anyway, it returns HTTP `409` with error code `errors.maintenance.merge_rejected`. Nothing is merged, and the merge-attempt count does not change.

To continue, choose one of these:

- **Record a new verdict on a newer report.** Save a new verification report for the same revision, then record a verdict on it. The rejection belongs to the older report, so it no longer blocks the merge.
- **Finish the change on GitHub.** Review and merge or close the pull request there.
- **Use a newer revision.** If someone pushed to the pull request, adopt its new head with **Use the new revision** (see below), then report on and review that revision.

After a new report and verdict, choose **Check fix status** in the review panel. **Finish this fix** is enabled again when the other merge checks also pass. Ordinary self-fix tasks without a maintenance request are not affected.

While Forge is finishing a fix, the review panel shows **Finishing started, not confirmed**. During that time, recording a verdict returns HTTP `409` with error code `errors.maintenance.merge_in_progress`. Choose **Finish this fix** to complete or confirm the merge, then record the verdict. A verdict recorded after the task reads **Finished** is accepted, because merging and acceptance are separate records. When the last finish attempt is unconfirmed, **Finish this fix** first reads the pull request on GitHub. A merged pull request is recorded as finished. A pull request closed without merging returns the task to review with changes requested. If the pull request is still open, Forge waits 30 seconds after the last attempt before trying again, and returns HTTP `409` with error code `errors.self_fix.merge_unconfirmed` until then. If no attempts remain, Forge keeps the task in this state and asks you to merge or close the pull request on GitHub, then choose **Finish this fix** to update the status. Forge assumes GitHub finishes or drops a merge request within those 30 seconds.

## When the pull request has a newer revision

Forge stores the pull request's head when it opens the pull request. If someone pushes to that pull request, the delivery panel reports that the head changed, Forge refuses acceptance for the old revision, and **Finish this fix** fails with "head moved". Refreshing evidence does not move the task to the new head. A platform administrator adopts it explicitly.

Before you start:

- You are a Forge platform administrator.
- The task's pull request is open on GitHub and still targets the branch the task was opened for.
- No finish attempt is running or unconfirmed (the review panel does not show **Finishing started, not confirmed**), and the change is not merged.

To adopt the newer revision:

1. Choose **Refresh evidence** in the task's **Verification, recovery and human review** panel.
2. The panel shows **The pull request has a newer revision**, with the recorded revision and the newer revision on GitHub (short forms).
3. Choose **Use the new revision**, check both revisions, then choose **Confirm: use revision …**.

Forge checks GitHub again before it writes anything. The pull request must be open, its base unchanged, and its head equal to the revision you confirmed. Forge then compares the pull request's base commit with the revision you confirmed. Both are fixed commit IDs, so retargeting or pushing to the pull request meanwhile cannot change what is checked. Afterwards Forge reads the pull request once more and refuses if it is no longer open, targets another base, or has another head. If any changed file is sensitive, or the comparison lists 300 or more files (GitHub lists at most 300, so the list may be incomplete), the task becomes **Needs owner or admin check** (`sensitive_blocked`) and in-app finishing stays disabled. A reconcile never clears an existing sensitive block.

Which files count as sensitive: the list covers the files that decide, authorize, record and wire self-fix delivery. That means the decision functions, the queries those decisions run, the administrator policy and its re-check query, the audit insert, the GitHub client (including its request and audit-text builders), the routes, their registration, the pull-request worker, the review reaper, both composition roots (the service wiring and the server binary), the files that declare the modules holding that code, and the authentication middleware. It also covers the dependency and build manifests (`Cargo.toml`, `Cargo.lock`, `build.rs`, `rust-toolchain.toml`, `rust-toolchain`, `package.json`, `package-lock.json`, and the `.cargo/` directories) and the vendored dependency source under `rust/third_party/`, because a change there can replace any code the checks depend on. Two safety limits can be configured, and both are bounded inside listed files, so configuration alone cannot remove them: `SELF_FIX_MAX_MERGE_ATTEMPTS` (1 to 10, in the self-fix service) and `SELF_FIX_REVIEW_DEADLINE_SECS` (1 hour to 30 days, in the review reaper). The merge settle window and the merge decision budget are fixed constants in the self-fix policy. Outside the list: shared row types, the database pool, the error and HTTP framework types, the shared orchestration repository file (a test keeps its module declarations plain), and deploy and container inputs such as `docker/`, `scripts/`, Makefiles and Dockerfiles, which are being added separately (issue #1319). That remaining risk is accepted because every self-fix change still needs an explicit administrator approval. The list is a tripwire that sends a change to a person, not the only layer of protection.

Success looks like this: the panel announces that the newer revision is recorded, and **Current recorded revision** shows the new head. An audit record names you and both revisions. Earlier reports and verdicts are kept, but they belong to the old revision and no longer apply. A rejection of the old revision no longer blocks finishing. A verdict always belongs to one revision: if the pull request later returns to a revision that already has a verdict, that verdict applies again (a rejection blocks finishing again, and an acceptance still needs your explicit **Finish this fix**). The merge-attempt count and the review deadline do not change.

What to do next: run or review the change at the new revision, save a new verification report, and record a verdict on it. Acceptance requires that new report.

If the action is refused, the API returns HTTP `409` with one of these codes:

| Code                                              | Meaning                                                                          | What to do                                                           |
| ------------------------------------------------- | -------------------------------------------------------------------------------- | -------------------------------------------------------------------- |
| `errors.self_fix.reconcile_stale`                 | The recorded revision changed since the page loaded                              | Refresh evidence, check both revisions, try again                    |
| `errors.self_fix.reconcile_head_mismatch`         | The pull request changed while Forge checked it (new revision or rewritten base) | Refresh evidence to see the newest revision, try again               |
| `errors.self_fix.reconcile_pull_request_not_open` | The pull request is closed or merged on GitHub                                   | Reopen it on GitHub, or finish the change there                      |
| `errors.self_fix.reconcile_base_changed`          | The pull request targets another repository or branch                            | Finish the change on GitHub                                          |
| `errors.self_fix.reconcile_merged`                | The change is already merged                                                     | Nothing to adopt                                                     |
| `errors.self_fix.reconcile_merge_unconfirmed`     | A finish attempt is in progress or not confirmed                                 | Choose **Finish this fix** to complete it, then use the new revision |
| `errors.self_fix.merge_running`                   | Another operation on this change is running                                      | Wait a moment, then try again                                        |

If GitHub cannot be read at any step, or returns something Forge cannot use, the API returns HTTP `503` with error code `errors.self_fix.reconcile_provider_unavailable` and nothing is written: the revision, status and audit log stay unchanged. Try again. If it keeps failing, check that the pull request still exists and that the GitHub App can access the repository. Another organization's task returns `404`, and a caller who is not a platform administrator gets `403`.

## Interpreting records and totals

The report’s **reported checks** are operator-entered claims and evidence. **Observed GitHub checks** are a separate provider snapshot tied to a revision and check time. The human verdict is a third record. A GitHub observation does not establish that a reported command ran, and a reported pass does not establish that GitHub checks passed.

Record the six human-minute categories as cumulative totals for the task across attempts: setup, handling, review, recovery, rework, and operation. Leave a category blank when it is unknown; enter `0` only when no time was spent. Each new verdict replaces the previous verdict’s minute totals for the task in outcome summaries, so the latest verdict is counted once. The optional baseline is a separately reported estimate for equivalent human work. A baseline comparison alone cannot establish time saved.

The optional Container CLI comparison uses a shared comparison reference and checks whether reports describe matching task conditions and distinct supported CLI tools. A conditions match is not a benchmark result and does not establish comparative quality or performance; review the reports and evidence before drawing conclusions.

## HTTP API and retry contract

All delivery endpoints require authenticated, live platform-administrator access and use the caller's organization. A task from another organization is not accessible. The API returns the standard `{ "ok": true, "data": ... }` envelope; the delivery read returns `null` for an ordinary task without a maintenance source.

| Method and path under `/api/v1`                            | Purpose                                                                                    |
| ---------------------------------------------------------- | ------------------------------------------------------------------------------------------ |
| `GET /self-fix/tasks/{id}/delivery`                        | Current revision, checks, recovery and bounded recent report/verdict/handoff history       |
| `POST /self-fix/tasks/{id}/reports`                        | Save a verification input and captured revision/run snapshot                               |
| `POST /self-fix/tasks/{id}/decisions`                      | Record a human verdict on the latest report for the current run/revision                   |
| `POST /self-fix/tasks/{id}/handoffs`                       | Save a blocker, next action and retained context for inactive work                         |
| `GET /self-fix/reports/{id}`                               | Retrieve a retained report and its latest human decision                                   |
| `GET /self-fix/outcomes`                                   | Query a submission cohort with `from`, `to`, optional `projectId` and detail-page `cursor` |
| `GET /self-fix/comparison?reportIds={comma-separated IDs}` | Compare 2–8 distinct reports and show conditions that do or do not match                   |

Writes include a UUID `requestKey`, `expectedVersion` and `expectedRevision`; reports also include the current `runId` (or `null` when no run exists). Retry an unconfirmed write with the same key and unchanged input. An acknowledged replay returns the original record even if the task has since changed. Reusing a key for different input, or submitting stale version/run/revision state, returns a conflict; refresh evidence before preparing a new record. A report without a produced change must explain the missing artifact. The full input and response contracts are in [maintenance-delivery.ts](../../shared/types/maintenance-delivery.ts).

Outcomes default to seven days and accept windows up to 120 days. Summary denominators cover the entire selected submission cohort; the 100-row detail cursor does not reduce them. Review activity during the period counts distinct tasks, including older submissions. Read and write requests in the browser expire after 30 seconds and are discarded when the account, organization or task changes.

## Finding older reports and retention

The task panel shows a bounded recent history. To retrieve an individual report and its latest human decision, an authenticated platform administrator can use `GET /api/v1/self-fix/reports/{id}` with the report ID.

If a run or agent is explicitly removed, its report metadata and captured database
snapshot are retained; the report does not copy raw artifact bytes into object
storage. Removing the live run link does not erase the recorded report.
Analytics and finished-run retention settings default to `0` (keep forever);
configured database sweeps do not delete attachment object bytes. An authorized
attachment deletion removes its object and then its metadata row. Workspace
files follow the operator's backup and cleanup process. The sidecar's local WAL
and assignment inbox support delivery and recovery, not archival: acknowledgment
removes pending WAL payloads and completed assignment tombstones expire. Archive
required raw evidence before deliberate task deletion or cleaning temporary
validation data. A general artifact-storage retention policy and its deletion
boundaries remain unqualified.

Migration 104 corrects the source-record foreign keys without changing migration 101. Explicit administrative deletion of a task then removes its maintenance source, reports, decisions, and handoffs. It does not add a task-deletion API or a complete organization-purge workflow; other existing organization references still follow the deployment's deletion policy. Back up required evidence before deliberate deletion.

# PR And CI Status Summary

Use a status snapshot when you need to know whether a pull request, merge
request, or CI pipeline needs human action without reading every remote status
line.

The rule is simple: take one compact snapshot, classify the result, then stop
unless there is something specific to fix.

## Prerequisites

- For GitHub pull requests, install the GitHub CLI: `gh`.
- For GitLab merge requests or pipelines, install the GitLab CLI: `glab`.
- Install `jq` for the JSON examples.
- Log in once with the CLI you use: `gh auth login` or `glab auth login`.
- Run the command from the repository checkout.

## GitHub Quick Check

```bash
npm run pr:summary
```

By default the command reuses a local GitHub snapshot for 15 minutes. Running it
again inside that window does not call GitHub again, so an agent can summarize
PR state without repeatedly spending chat/output budget on the same remote
checks. This is a point-in-time status tool, not a live watch command.

Success looks like this:

```text
[pr-summary] fresh GitHub snapshot saved; use npm run pr:summary:local or cached npm run pr:summary for the next 15m; repeat remote reads are blocked for 1m
[pr-summary] ACTION 0 | WAIT 42 | DONE 0
ACTION: none
WAIT: 42 PR(s) waiting on review, CI, draft state, or merge queue
WAIT: use --show-wait to list them when a human needs the full queue
WAIT: stop here; use npm run pr:summary:local until cache expiry or a known remote change
WAIT: token-safe action: do not poll in chat; use scheduled monitoring for the next check
```

When the output says it used a cached snapshot, that is expected. Treat the
result as a recent point-in-time view, not a live watch. The cache notice also
tells you when another remote read would be useful again.

If the instruction is "do not poll" or "do not read remote again", use the
local-only command:

```bash
npm run pr:summary:local
```

This command never calls GitHub. It reads the last saved snapshot, prints how
old it is, and tells you when a fresh remote read is needed. If no saved
snapshot exists yet, run `npm run pr:summary:refresh` once only when a fresh
remote read is acceptable.

When the output says `WAIT: stop here` or `WAIT: token-safe action`, the correct
next step is to leave the conversation or monitor quiet. If you only need to
repeat the same view, use `npm run pr:summary:local`; do not ask an agent to
refresh remote state again unless the cache has expired or someone pushed,
approved, failed, or merged the PR.

## Do Not Poll In Chat

Use this command as a snapshot, then stop. A `WAIT` result means GitHub is
already handling review, CI, or merge queue work. Ask an agent to check again
later, or schedule the command outside the chat.

Operator rule: `WAIT` is a valid end state for the conversation. Do not keep
asking the agent to refresh the same PR queue, and do not run `gh pr checks
--watch`, `gh run watch`, shell loops, or repeated forced refreshes from the
chat. Those commands spend tokens on unchanged remote state without creating
new work.

Safe rhythm for most teams:

1. Run `npm run pr:summary`.
2. Fix only PRs listed under `ACTION`.
3. Leave `WAIT` PRs alone unless a reviewer asks for the full list.
4. Re-run after 10 to 15 minutes, or after a known remote change such as a new
   push.

Agent rule: after a `WAIT` snapshot, do not ask the agent to keep checking in
the conversation. The next useful check is either the cache expiry time printed
by the script or a real remote change, such as a push, review, failed check, or
merge.

If a human wants background monitoring, use `npm run pr:summary:monitor` from a
scheduled job. The chat should only receive the compact result when `ACTION`
appears or when someone explicitly asks for a new snapshot. The monitor command
keeps a 1-hour snapshot by default so a short scheduler interval does not become
a hidden polling loop.

## GitLab Quick Check

Use GitLab checks the same way: one compact snapshot, then classify the result
as `ACTION`, `WAIT`, or `DONE`.

For a merge request:

```bash
glab mr view --output json \
  | jq -r '[
      "state="+.state,
      "sha="+.sha,
      "merge_status="+(.detailed_merge_status // .merge_status // "unknown"),
      "pipeline_status="+(.head_pipeline.status // "none"),
      "url="+.web_url
    ] | .[]'
```

For a standalone pipeline:

```bash
glab pipeline list --ref <branch> --per-page 5
```

Use the snapshot this way:

- `ACTION`: the pipeline failed, was canceled, needs a manual job, has a merge
  conflict, or shows another concrete blocker. Fetch only the failed job list
  and the shortest useful trace tail.
- `WAIT`: the pipeline is running, pending, queued, or already covered by
  merge-when-pipeline-succeeds. Stop checking in chat.
- `DONE`: the merge request is merged, or the pinned pipeline finished
  successfully.

Do not run GitLab watch mode, shell loops, or repeated `glab pipeline list`
refreshes from the chat. If a merge request should merge after CI passes,
enable GitLab's server-side merge-when-pipeline-succeeds and leave the
conversation at `WAIT` unless someone asked for a bounded final answer.

When a failed GitLab job needs investigation, keep the output small:

```bash
glab api "projects/<project_id>/pipelines/<pipeline_id>/jobs?per_page=100" \
  | jq -r '.[] | select(.status=="failed" or .status=="canceled" or .status=="manual") | [.id,.name,.stage,.status,.failure_reason,.web_url] | @tsv'

glab api "projects/<project_id>/jobs/<job_id>/trace" | tail -n 160
```

Use a local bounded waiter only when a human explicitly asks for a definitive
merged, passed, or blocked answer. The waiter should print nothing while the
remote state is merely pending, print one terminal JSON result, and stop on a
timeout. Do not leave a waiter running after the chat response is sent.

## Refresh Only When Needed

Use a refresh when you just pushed a fix, enabled auto-merge, or need a new
point-in-time answer:

```bash
npm run pr:summary:refresh
```

`refresh` still has a short safety cooldown. If someone runs the command again
right away, it reuses the latest snapshot instead of calling GitHub again.

Use a forced refresh only when you know the remote state changed and the command
must ignore the normal cooldown:

```bash
npm run pr:summary:force-refresh
```

Even forced refresh keeps a 60-second repeat-read guard for the same query. That
prevents an agent, shell loop, or impatient operator from hitting GitHub over and
over while nothing useful changed.

When you only want to reuse what was already checked, do not refresh:

```bash
npm run pr:summary:local
```

The local-only command will show a stale snapshot if that is all it has, but it
will not make a hidden remote request.

If an emergency manual check must read GitHub again immediately, make that
choice explicit:

```bash
npm run pr:summary:force-refresh -- --allow-repeat-remote-read
```

Do not put that emergency flag in scripts, aliases, scheduled jobs, or agent
skills.

Use a shorter or longer reuse window when an operator has a reason to change
the default:

```bash
npm run pr:summary -- --cache-ttl-seconds 300
```

Change the repeat-read guard only for a one-time manual session:

```bash
npm run pr:summary:refresh -- --min-remote-read-interval-seconds 120
```

The command refuses values below 60 seconds unless you also pass the emergency
`--allow-repeat-remote-read` flag. That keeps agents, aliases, and shell loops
from turning a status check into repeated GitHub reads.

Do not put `npm run pr:summary:refresh` or `npm run pr:summary:force-refresh`
in a tight loop. This command is a snapshot tool, not a chat-based live watch.
For monitoring, schedule `npm run pr:summary:monitor` at a fixed interval such
as hourly and alert only when the `ACTION` count is greater than zero. If the
monitor runs more often, it should still reuse the 1-hour local snapshot instead
of reading GitHub every time.

`--no-cache` is intentionally blocked unless you also pass
`--allow-repeat-remote-read`. It removes the local protection that stops repeated
remote reads, so keep it for one-time troubleshooting only.

## What The Buckets Mean

- `ACTION` means a person or agent should intervene. Examples: failed checks,
  merge conflicts, requested changes, or auto-merge not enabled.
- `WAIT` means the PR is already in GitHub's hands. Examples: waiting for
  review, pending CI, draft state, or merge queue.
- `DONE` means the PR is already closed or merged.

## Show The Wait Queue

Use this only when a reviewer asks for the full queue:

```bash
npm run pr:summary -- --show-wait
```

## Automation Mode

Use `--fail-on-action` when a job should only alert on actionable PRs:

```bash
npm run pr:summary -- --fail-on-action
```

The command exits with status `1` only when one or more PRs are in `ACTION`.
That keeps monitors quiet while PRs are merely waiting for review or CI.

For a low-noise monitor, schedule this command hourly:

```bash
npm run pr:summary:monitor
```

The monitor command prints one compact summary, exits cleanly for `WAIT` and
`DONE` states, and only fails when a person or agent has something specific to
fix. It uses a 1-hour snapshot reuse window, so a monitor scheduled too
frequently still reads cached state instead of repeatedly calling GitHub.

The monitor command also rejects refresh bypasses such as `--refresh`,
`--force-refresh`, `--no-cache`, `--allow-repeat-remote-read`, or a cache window
shorter than 1 hour. If you need an immediate one-time answer, run the
manual refresh command from the previous section instead of changing the
scheduled monitor.

## Offline Review

The script can read a saved `gh pr list` JSON file:

```bash
gh pr list --state open --limit 120 \
  --json autoMergeRequest,headRefName,isDraft,mergeStateStatus,number,reviewDecision,state,statusCheckRollup,title,url \
  > /tmp/prs.json

npm run pr:summary -- --input /tmp/prs.json
```

Use the JSON form when another system already collected the PR state and you
want the agent to read only the compact summary.

# PR Status Summary

Use the PR summary script when you need to know whether any pull request needs
human action without reading every GitHub check line.

## Prerequisites

- Install the GitHub CLI: `gh`.
- Log in once: `gh auth login`.
- Run the command from the repository checkout.

## Quick Check

```bash
npm run pr:summary
```

By default the command reuses a local GitHub snapshot for 15 minutes. Running it
again inside that window does not call GitHub again, so an agent can summarize
PR state without repeatedly spending chat/output budget on the same remote
checks.

Success looks like this:

```text
[pr-summary] ACTION 0 | WAIT 42 | DONE 0
ACTION: none
WAIT: 42 PR(s) waiting on review, CI, draft state, or merge queue
WAIT: use --show-wait to list them when a human needs the full queue
```

When the output says it used a cached snapshot, that is expected. Treat the
result as a recent point-in-time view, not a live watch.

## Refresh Only When Needed

Use a refresh when you just pushed a fix, enabled auto-merge, or need a new
point-in-time answer:

```bash
npm run pr:summary:refresh
```

`refresh` still has a short safety cooldown. If someone runs the command again
right away, it reuses the latest snapshot instead of calling GitHub again.

Use a forced refresh only when you know the remote state changed and the command
must ignore that cooldown:

```bash
npm run pr:summary:force-refresh
```

Use a shorter or longer reuse window when an operator has a reason to change
the default:

```bash
npm run pr:summary -- --cache-ttl-seconds 300
```

Do not put `npm run pr:summary:refresh` or `npm run pr:summary:force-refresh`
in a tight loop. This command is a snapshot tool, not a chat-based live watch.
For monitoring, schedule it at a fixed interval such as 10 or 15 minutes and
alert only when the `ACTION` count is greater than zero.

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

For a low-noise monitor, run:

```bash
npm run pr:summary:refresh -- --fail-on-action
```

The command prints one compact summary, exits cleanly for `WAIT` and `DONE`
states, and only fails when a person or agent has something specific to fix.
Running the monitor again too soon uses the cached snapshot, which prevents
accidental repeated polling from wasting operator time or agent context.

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

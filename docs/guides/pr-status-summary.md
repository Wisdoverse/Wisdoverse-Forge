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

Success looks like this:

```text
[pr-summary] ACTION 0 | WAIT 42 | DONE 0
ACTION: none
WAIT: 42 PR(s) waiting on review, CI, draft state, or merge queue
WAIT: use --show-wait to list them when a human needs the full queue
```

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

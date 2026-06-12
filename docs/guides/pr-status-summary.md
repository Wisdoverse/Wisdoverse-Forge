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
checks. This is a point-in-time status tool, not a live watch command.

Success looks like this:

```text
[pr-summary] ACTION 0 | WAIT 42 | DONE 0
ACTION: none
WAIT: 42 PR(s) waiting on review, CI, draft state, or merge queue
WAIT: use --show-wait to list them when a human needs the full queue
WAIT: stop here; refresh only after cache expiry or a known remote change
WAIT: token-safe action: do not poll in chat; use scheduled monitoring for the next check
```

When the output says it used a cached snapshot, that is expected. Treat the
result as a recent point-in-time view, not a live watch. The cache notice also
tells you when another remote read would be useful again.

When the output says `WAIT: stop here` or `WAIT: token-safe action`, the correct
next step is to leave the conversation or monitor quiet. Do not ask an agent to
refresh again unless the cache has expired or someone pushed, approved, failed,
or merged the PR.

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

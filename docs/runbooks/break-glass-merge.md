# Break-Glass Merge Runbook

Use this procedure when no eligible second reviewer is available.
It waives only the required approval under the repository policy.
Every required status check must succeed for the reviewed commit.

## Before You Start

- Install `gh`.
- Authenticate as a repository admin.
- Install Node.js 24.15.0 or later with npm.
- Use Bash on Linux or macOS.
- On Windows, use PowerShell 7.
- Use a checkout of the PR's repository.
- Confirm that the current rules permit an admin bypass through pull requests.
- Replace the quoted placeholders in the commands below.
- Choose a private snapshot path outside the repository.

The repository requires review, status checks, and linear history.
It prohibits force pushes, deletion of `main`, and direct admin pushes to `main`.
This procedure does not change those rules or repository permissions.

## Permission Boundary

Use the normal approval path when an eligible reviewer is available.
Use the approval waiver only when no eligible second reviewer is available.

The `--admin` flag can bypass repository requirements.
It does not enforce this procedure's restriction to the approval requirement.
Do not use it to bypass failed, missing, or incomplete required checks.

Skipped or neutral advisory contexts do not waive a required check.
Resolve any latest failed or canceled check before merging.

## Review One Snapshot

Capture one PR snapshot with the full head SHA and check results.
Use this command block in Bash:

```bash
gh pr view '<PR number>' \
  --json number,state,baseRefName,headRefOid,isDraft,reviewDecision,mergeStateStatus,statusCheckRollup,body \
  --jq '[.]' > '<private snapshot path>'
npm run pr:summary -- --input '<private snapshot path>' --json
```

On Windows, use this command block in PowerShell 7:

```powershell
gh pr view '<PR number>' --json number,state,baseRefName,headRefOid,isDraft,reviewDecision,mergeStateStatus,statusCheckRollup,body --jq '[.]' | Set-Content -LiteralPath '<private snapshot path>' -Encoding utf8NoBOM
npm run pr:summary -- --input '<private snapshot path>' --json
```

The second command reads the saved snapshot without another provider request.
Use `headRefOid` from this snapshot as the reviewed full SHA.
Confirm that the PR is open, ready for review, and targets `main`.

Read the current target rules instead of relying on a fixed check count:

```bash
gh api 'repos/<owner>/<repo>/rules/branches/main' --jq '.[] | .parameters.required_status_checks[]?.context'
```

Match every required context to its latest successful result for the snapshot's head.
Stop if any required context is missing, incomplete, or unsuccessful.
Resolve any reported check failure before proceeding.
Confirm that the PR body has a complete **Beginner UX / First-Time User Path** section.
Confirm that the approval requirement is the only remaining merge gate.

## Merge The Reviewed Head

Use the full SHA from the reviewed snapshot:

```bash
gh pr merge '<PR number>' --squash --admin --match-head-commit '<reviewed full SHA>'
```

GitHub refuses this command if the PR head has changed.
Validate the new head before another merge attempt.

After merge, verify the merged source head and its integration evidence.
Confirm ownership, a clean worktree, and no open dependent PRs before cleanup.
Follow the [workflow guide](../agents/workflow.md#pull-requests) when removing the owned source branch or worktree.

## Record The Decision

Prepare an audit comment that records these facts:

- Why no eligible second reviewer was available.
- The reviewed full source SHA and target branch.
- Every required check's successful result.
- Any skipped or neutral advisory contexts.
- The approval waiver and the absence of any CI waiver.
- A link to this runbook.

Post the prepared comment after GitHub confirms the merge:

```bash
gh pr comment '<PR number>' --body-file '<audit comment path>'
```

The squash commit references the PR number.
The PR retains its check history and audit comment.
The ruleset bypass history records the actor and timestamp.

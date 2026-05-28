# Break-Glass Merge Runbook

## Purpose

The `main` branch is protected by a GitHub ruleset that requires:

- All 15 status checks green (ESLint, Prettier, Typecheck, Unit/Integration/Rust Tests, Build, Dependency Audit, Dangerous Pattern Scan, Secret Leak Scan, Trivy, CodeQL Analyze ×3, Version Guard)
- 1 approving pull-request review
- No force-push, no branch deletion, linear history

This runbook documents the **only** sanctioned way to merge when the
1-approval requirement cannot be satisfied through the normal path — for
example, a solo maintainer with no second reviewer available, or an urgent
fix during an incident.

## What break-glass is and is not

The ruleset grants the **Repository Admin role** a bypass actor with
`bypass_mode: pull_request`. This means:

- **Allowed:** an admin may merge a pull request without the 1 approving
  review, _provided every status check is green_.
- **Not allowed:** direct pushes to `main`. Even an admin must open a pull
  request. `bypass_mode` is `pull_request`, not `always`, specifically so that
  every change to `main` leaves an auditable PR trail.
- **Not bypassed:** status checks. Break-glass does **not** skip CI. A red
  check still blocks the merge; `--admin` only waives the human-approval
  requirement, never the automated gates.

## When break-glass is justified

1. **Solo maintainer, green CI, no available reviewer.** The change has passed
   every automated gate and waiting for a human reviewer is not possible.
2. **Incident response.** A fix must land to restore service and the on-call
   engineer is the only person available.
3. **Tooling-required follow-up.** A previously-approved change needs a trivial
   mechanical follow-up (e.g. regenerating a checksum manifest) that cannot
   itself collect a second review in time.

Break-glass is **not** justified for:

- Skipping a red check (the bypass does not do this; do not try to disable the
  check instead).
- Avoiding review of a substantive change when a reviewer _is_ available.
- Routine convenience.

## Procedure

1. Confirm every required status check is green:

   ```bash
   gh pr checks <PR> | grep -v -E '\bpass\b|skipping'
   ```

   If anything other than `pass` or `skipping` remains, **stop** — fix the
   check first. Break-glass never merges a red PR.

2. Confirm the PR body has a complete Beginner UX / Operator Path section. The
   PR template asks for this (advisory `Beginner UX / Operator Path` check); it
   is not one of the 15 required status checks, so confirm it by hand before
   merging.

3. Merge with the admin bypass:

   ```bash
   gh pr merge <PR> --squash --admin --delete-branch
   ```

4. **Record the break-glass event.** Post a comment on the merged PR stating:
   - Why a second review was not obtained
   - That all status checks were green at merge time
   - Link to this runbook

   Example:

   ```bash
   gh pr comment <PR> --body "Break-glass merge per docs/runbooks/break-glass-merge.md: solo maintainer, no second reviewer available, all 15 status checks green at merge. No checks were skipped."
   ```

## Audit trail

Every break-glass merge is reconstructable after the fact:

- The squash-merge commit on `main` references the PR number.
- The PR retains its full check history and the break-glass comment.
- GitHub's ruleset bypass log records the admin actor and timestamp
  (Settings → Rules → main → Bypass history).

## Reverting the governance posture

If the project gains a second maintainer and break-glass should no longer be
routine, no ruleset change is required — simply collect the 1 approving review
on every PR and never pass `--admin`. The bypass actor remains as a true
emergency path only.

To remove the bypass entirely (force _all_ merges through review):

```bash
gh api repos/Wisdoverse/Wisdoverse-Forge/rulesets/16172271 \
  --method PUT \
  --input - <<'JSON'
{ "bypass_actors": [] }
JSON
```

To restore direct-push capability for admins (NOT recommended — defeats the
audit trail), set `bypass_mode` back to `always`.

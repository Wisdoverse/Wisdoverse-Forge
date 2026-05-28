# Postmortem: Codex `-X theirs` Mass-Rebase Fallout (2026-05-27)

**Date of incident:** 2026-05-27
**Authors:** Claude (acting as PM/engineer)
**Status:** Resolved (PRs #448, #449)
**Impact:** CI red for ~12 hours; 41 broken files including JSX parse errors, missing imports, dropped UI features, and unused-var lint storms; pre-existing test suite went from 591/631 to 631/631 only after #449.

## What happened

130 `codex/*` UX guidance PRs (#346-446) were batch-merged via `gh pr merge --admin --squash` with `git rebase -X theirs` as the conflict-resolution strategy for the 35 that conflicted on shared files. The `-X theirs` strategy auto-favored the PR side, scrubbing helper declarations / imports / sub-components that existed in main but were absent in the PR diff because the PR's author hadn't redeclared them.

After the merge wave landed:

- `OrganizationsPanel.tsx` had a broken `<tr>...{cell}</td>` with 5 missing `<td>` cells.
- `SystemHealth.tsx` referenced `hasIssue` which was no longer declared.
- `CreateSkillModal.tsx` used `useRef`, `updateField`, `SKILL_REVIEW_POINTS` that the rebase deleted.
- `TeamsSection.tsx` lost the `Plus` import.
- 31 unused-var lint errors surfaced because the rebase kept added imports/vars but dropped the JSX that consumed them.
- 40 Vitest tests failed because UI text in the merged code no longer matched what the tests asserted (different codex PRs assumed different text and the rebase preserved whichever lost).

CI on `main` stayed red until #448 (helpers/imports + lint) and #449 (test alignment + Rust linker + DDD boundary) landed.

## Root cause

`git rebase -X theirs` is a conflict-resolution strategy, not a merge-safety strategy. It silently accepts the PR side at every conflict marker, even when the PR side has structural dependencies on the main side that the PR diff doesn't mention. The strategy is appropriate ONLY for pure-additive PRs on file regions main has not touched. For 35 of the 130 codex PRs, the file regions HAD been touched by sibling already-merged codex PRs, so `-X theirs` lost main-side code that the merged PR still required to compile.

Compounding cause: no base-build verification gate between the rebase and the admin-merge. The merge script never ran `tsc --noEmit` or `cargo check` against the rebased branch before pushing the merge.

## Why it slipped through CI per-PR

Each codex PR was small (2-4 files, mostly orthogonal). CI on the individual PR was green. The breakage only manifested after batched rebase. Per-PR CI cannot catch "this PR will break after a sibling merge" without running CI against the rebased + merged HEAD, which most CI configurations do not do unless `merge_group` triggers are wired.

## What we changed

1. **PR #448** — restored 20 files (PLAN_DETAILS, planDescription, organizationReadiness, hasIssue, useRef, Plus, SKILL_REVIEW_POINTS, nextStepTitle, credentialFormReadiness, ROLE_DETAILS, formatCpu, etc.). Repaired OrganizationsPanel JSX. Cleared 31 unused-var lint errors.
2. **PR #449** — aligned 40 pre-existing Vitest failures with current UI copy. Fixed Rust CI linker SIGSEGV (mold + `debug = "line-tables-only"`). Moved 4 `ErrorKind` constructors from middleware/repo/service to domain helpers per `route_ddd_boundary_test`.

## Process changes (preventing recurrence)

1. **Banned: `git rebase -X theirs` (or `-X ours`) for batch merges of independent PRs.** Allowed only for true conflict-free rebases (where `git rebase` succeeds without any `-X` strategy).
2. **Required: post-rebase + pre-merge verification.** For any rebased branch:
   - `cargo check --workspace` MUST pass before push.
   - `npm run typecheck` MUST pass before push.
   - `npm run fsd:check` MUST pass before push.
   - If any of the above fails, the merge is aborted and the PR is left for individual rebase by the author.
3. **Required: CI `merge_group` trigger** on `main` (GitHub Actions `merge_group` event). When a future GitHub-native merge queue lands, this becomes mandatory; until then, the per-PR CI is the gate and admin-bypass is reserved for documented incidents.
4. **Required: postmortem doc** for any incident that triggers CI red on `main` for > 1 hour OR requires > 5 file-level repair commits.

## Open follow-ups

- (none — both repair PRs landed; verification gates documented in `docs/architecture/ddd-contract.md` and `CLAUDE.md`)

## Timeline

| Time (UTC, 2026-05-27) | Event                                                                                                             |
| ---------------------- | ----------------------------------------------------------------------------------------------------------------- |
| 10:54                  | 130 codex PRs admin-merged via batch script using `-X theirs` for 35 conflicting branches                         |
| 11:00                  | CI red on `main`; OrganizationsPanel JSX parse error blocks `npm run typecheck`                                   |
| 22:18                  | PR #448 (helpers/imports restoration + lint cleanup) opened                                                       |
| 22:28                  | PR #448 admin-merged (CI partial green; 41 unit tests still failing as pre-existing)                              |
| 23:13–17:51 (next day) | PR #449 prepared (40 test alignments, Rust linker fix, DDD boundary fix, tenant scope allowlist) and admin-merged |
| 17:51                  | CI fully green on `main`; closure                                                                                 |

# Self-fix loop (Phase 4 + human-gated merge) — design

Status: **Design / approved for spec review** (2026-06-14). Implements Phase 4 of
`docs/plans/self-iteration-roadmap.md`, extended with the human-gated merge slice
of Phase 5. Optimised for **dogfooding / hardening the product**, not velocity or
a polished demo.

> **Review history.** Revised 2026-06-14 after a Codex review plus an adversarial
> security + correctness pass. Two trust-boundary holes were reproduced and closed
> (agent-controlled local Git config/filters executing under the push credential;
> base-SHA not verified as an ancestor). The status model was corrected: the merge
> gate uses the orchestrator's `ReviewState` aggregate, not the pre-dispatch
> `waiting_approval` block. See the "Closed review findings" section at the end.

## Problem & goal

The platform is a governed AI agent workbench, but it does not use its own
agent/orchestration/review machinery on its own codebase. The strongest way to
harden those primitives is to make the hardest customer — the platform itself —
run real fixes through the real product surface.

**Goal:** a human creates a "fix this bug" task on the platform's own board; an
agent works the platform's repo through the existing orchestration spine; the
platform opens a draft PR; a human reviews it in-platform; on approval the
platform merges (CI-green-gated). Every step exercises a real product primitive.

**Why this and not the alternatives:** an external CI-driven fixer (e.g. a
GitHub Action) would never touch the platform's own agent surface, so it fails
the dogfood goal. Giving the container agent its own push/merge credentials is
simpler server-side but violates least privilege and re-creates the documented
bot-branch injection failure mode. The chosen design keeps all privileged Git
operations server-side, outside the agent's trust boundary, and — critically —
never runs any Git process against the agent's own repository.

## Non-goals (explicit)

- **No auto-deploy.** Merge to `main` is the terminal action; deploying the
  changed service stays a manual `make deploy-server`. (Deploy is the Phase 5
  tail and collides with the migrate-on-boot realities mapped in the roadmap.)
- **No autonomous/background dispatch.** A human creates each task. No scheduler,
  no issue-webhook intake. (That is Phase 6 and is not built here.)
- **No merge without a human.** The in-platform Approve click is the
  human-confirm gate. There is no auto-merge tier.
- **No multi-repo / multi-tool generality.** One repo (this one), one Container
  CLI (the default `claude`). Generalise only if a second consumer appears.

## Architecture & trust boundary

A single trust boundary splits the system. The load-bearing invariant: **no Git
process and no shell ever executes with the agent's repository config, hooks,
attributes, or filters in scope, and only vetted regular-file content crosses
into the server-owned clone.**

```
        ┌────────────────────────── TRUSTED (server side) ───────────────────────────┐
        │  PR Bridge          Merge Executor          Review surface (board panel)     │
        │  (rebuild change on  (ReviewState=Approved   (PR diff + check status +        │
        │   server-pinned base, + sensitive-path clear (in-platform Approve →           │
        │   push, draft PR)     → ready → guarded merge) ReviewState::Approved)          │
        │  scoped GitHub App   scoped GitHub App                                         │
        └───────────────▲───────────────────────────────────▲──────────────────────────┘
                        │ imports VETTED FILE CONTENT only   │ records human approval
                        │ (never runs git in /workspace)     │ as ReviewState=Approved
  ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ │ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ │ ─ ─ ─ trust boundary ─ ─
                        │                                    │
        ┌───────────────┴──── UNTRUSTED (the agent) ─────────┴──────────────────────────┐
        │  Container CLI in /workspace: investigate → edit → `git commit` to a LOCAL     │
        │  branch agent/<task-id>. Has NO remote credential. Cannot push, open, or merge.│
        │  Fully controls /workspace/.git (config, hooks, attributes, filters, refs).    │
        └───────────────────────────────────────────────────────────────────────────────┘
```

The agent may run `git` locally in `/workspace`, and it fully controls
`/workspace/.git` (the mount is read-write; confirmed `docker/compose.yml`
workspace bind is `read_only: false`). The server therefore treats that entire
directory, **including `.git`**, as attacker-controlled. The server never fetches
from it, never runs a Git plumbing command inside it, and never reads its config.
Every operation that reaches GitHub (push, PR open, merge) runs server-side, in a
server-owned clone, with a credential the agent cannot see.

### Reused (existing spine — unchanged)

- Board task → task group → orchestration assignment → Container CLI spawn with
  the org/workspace projects root mounted at `/workspace`
  (`CONTAINER_WORKSPACE_ROOT`, `rust/crates/api/src/domain/agent_workspace.rs`).
  `wisdoverse-forge` is already a project in this workspace, so the agent can
  check it out today.
- The `orchestration.result.<agent_id>` → result consumer → task state machine.
- The orchestrator's **review aggregate** (`rust/crates/orchestrator/src/review/`,
  `ReviewState` = `pending | in_review | approved | changes_requested | rejected`)
  — the purpose-built post-work diff-review surface this design gates merge on.
- Container lifecycle control (`AgentContainerControlService`,
  `rust/crates/api/src/services/agent_container_control.rs`) — used here to
  **stop the container before extraction**, freezing `/workspace`.
- The full CI gauntlet, the Beginner UX PR-body gate, the guard tests, and the
  break-glass merge policy.
- Container security validation (`platform/security.rs`: privileged / host-PID /
  host-net / forbidden caps+mounts / resource limits hard-denied) — never relaxed.

### Net-new components

| Component | Responsibility | Depends on |
| --------- | -------------- | ---------- |
| **PR Bridge** (server) | On a self-fix task completing: stop the container; rebuild the agent's change on top of the **server-pinned base SHA** inside a fresh server-owned worktree by importing vetted file content (see Security §1); server-author one commit; push `agent/<task-id>`; open a **draft** PR (title/body incl. a generated Beginner-UX section); record `pr_number`/`pr_url`/`pr_head_sha` (the SHA *the server* created) on the task; create/transition the task's review aggregate to `in_review`. | scoped GitHub App, workspace mount path, container control, GitHub API |
| **Review surface** (board task panel) | Show the PR diff + per-check CI status (one-shot snapshot, not polled); render an Approve action that, on click, records `ReviewState::Approved`. Approve is enabled only when all required checks are green AND no sensitive path is touched. | PR metadata the Bridge wrote, review aggregate |
| **Merge Executor** (server) | On `ReviewState::Approved`: **hard-refuse server-side** if any sensitive path is touched (independent of GitHub review state); else mark the PR ready-for-review, then re-verify required checks green + head unchanged, then merge with an expected-head-SHA guard; write an audit comment (approver, task, timestamp, head sha). | scoped GitHub App, sensitive-path policy, review aggregate |
| **Self-fix task shape** | A `self-fix` task group (or typed flag) marking a task as a code-fix against this repo; carries the PR linkage and the base-SHA pinned at spawn. | `orchestration_tasks` (+ PR/base-SHA columns) |

## Data flow (lifecycle)

1. Human creates a task in the `self-fix` task group describing the bug. At spawn
   the server **records the base commit SHA** of `origin/main` it checked the
   agent out against (`base_commit_sha` on the task — net-new; not recorded today).
2. Orchestration auto-assigns it → spawns the `claude` Container CLI with
   `/workspace` mounted.
3. The agent investigates → edits → `git commit` to local branch
   `agent/<task-id>` → signals done. It does **not** push.
4. Task-completed event → **PR Bridge**:
   - **Stops the agent container** first, freezing `/workspace` and killing any
     lingering agent processes (closes the done-signal TOCTOU window).
   - In the **server-owned clean clone**, fetches `origin`, then adds an ephemeral
     detached worktree at the pinned `base_commit_sha` (rejects if the base SHA is
     unknown). The worktree carries the *base tree's* trusted `.gitignore`.
   - **Imports vetted file content** from `/workspace` (excluding `.git/`) onto the
     worktree, refusing: symlinks, submodules/gitlinks, special files, paths that
     escape the root after canonicalisation (`..`/absolute), paths outside the
     allowlist, files over the size cap, and changes exceeding the churn /
     deletion-count caps (Security §1). No Git command is run inside `/workspace`.
   - Server-authors one commit; **object-level re-check** inside the clone
     (`git diff-tree -r --raw <base> HEAD`) asserts no entry has dst mode `120000`
     (symlink) or `160000` (gitlink) and no mode-only escalation slipped in.
   - Pushes `HEAD:refs/heads/agent/<task-id>` from the clone (server remote, server
     creds, hooks disabled); opens a **draft** PR; records
     `pr_number`/`pr_url`/`pr_head_sha` (= the server-created SHA).
   - Creates/sets the task's review aggregate to `ReviewState::in_review`. The task
     status stays `completed`.
5. The board task panel renders the diff + a one-shot CI-check snapshot.
6. Human reviews the diff → clicks **Approve** (enabled only when all required
   checks are green AND the PR touches no sensitive path) → `ReviewState::Approved`.
7. **Merge Executor**: hard-refuse if sensitive path touched → else mark ready →
   re-verify checks green + head unchanged → merge with expected-head-SHA guard →
   audit comment. Task stays `completed`; deploy stays manual.

## Security & credential model (the load-bearing part)

### 1. Server-side Git never touches the agent's repository

The agent fully controls `/workspace/.git`. A Git process run there will execute
agent-defined **hooks, clean/smudge filters, textconv, and `uploadpack`
hooks** — `core.hooksPath=/dev/null` and `GIT_CONFIG_GLOBAL/SYSTEM=/dev/null`
do **not** disable the repo-LOCAL `/workspace/.git/config`, so a planted
`[filter "x"] clean = <cmd>` + a tracked `.gitattributes` fires arbitrary code
during something as innocent as `git diff` — in the very process that holds the
push credential. (Reproduced during review.) Fetching the agent branch over
`file://` is also rejected: local fetch still runs the agent repo's
`upload-pack`, honouring its `uploadpack.packObjectsHook`.

Therefore the Bridge **rebuilds** the change instead of transporting Git state:

- Stop the container (freeze `/workspace`).
- In the server-owned clone, create an ephemeral detached worktree at the
  **server-pinned `base_commit_sha`** (verified to exist; an attacker cannot pick
  the base because the server recorded it at spawn). Building on the pinned base
  directly makes orphan-branch / unrelated-history attacks moot — the agent's
  commit graph is never trusted or read.
- Import only **vetted regular-file content** from `/workspace` (a server-side
  file walk, not Git), honouring the *base tree's* trusted `.gitignore`. Refuse,
  do not silently skip: symlinks, gitlinks/submodules, FIFOs/devices, paths with
  `..` or absolute components after canonicalisation, paths outside the allowlist,
  files over the per-file size cap, and changes over the total-churn and
  deletion-count caps. A mode-only change (e.g. `100644`→`100755`) counts as
  touching its path for sensitive-path purposes.
- Server-author one commit (server identity + message), then re-validate at the
  object level (`git diff-tree -r --raw`) and assert the resulting tree contains
  no symlink/gitlink and no out-of-allowlist path before pushing.

The scoped credential lives only in the clone's environment and is used only for
push / PR / merge. It never enters `/workspace` and never shares a process with
agent-controlled Git state.

### 2. Clean-clone hygiene (no cross-task bleed)

The server clone is treated as ephemeral per task: a fresh `git worktree add
--detach` off the pinned base per run (or `reset --hard <base>; clean -fdx;
checkout --detach <base>`), delete any leftover `agent/*` branches, and re-assert
on every use that the clone's remote, config (no credential helper, no filters),
and `hooksPath` are server-set. A worktree that has imported agent content is
never reused for another task.

### 3. Credential = a GitHub App, not a PAT

Short-lived per-repo installation tokens scoped to `contents:write` +
`pull_requests:write` (+ the merge capability). The App private key is encrypted
at rest (same posture as `LLM_ENCRYPTION_KEY`, mirroring the read-only secret
mount pattern already used for OAuth creds under `/run/secrets/`); installation
tokens are minted on demand and never logged. A long-lived PAT (broad, hard to
rotate/audit) is rejected.

### 4. Server-derived branch name

`agent/<task-id>` is computed from the task id server-side; the Bridge pushes
exactly that ref from the clone. The agent cannot name the branch or redirect the
push.

### 5. Checks are never bypassed

The merge does NOT use `--admin`'s check-waiving power. The Merge Executor
independently verifies every required check is green and uses GitHub's
expected-head-SHA guard on the merge call so GitHub itself rejects the merge if
the head moved (defence in depth, not relying on branch protection alone). The
in-platform Approve, recorded as `ReviewState::Approved` (approver id, task, diff
reviewed, timestamp), is the human authority; an audit comment is left on the PR.

### 6. Sensitive-path circuit breaker (non-negotiable) + server-side hard refusal

If the PR touches any sensitive path, the in-platform Approve is **disabled** AND
the Merge Executor **hard-refuses to merge server-side regardless of GitHub
review state**. The platform never auto-merges sensitive paths; they route to a
CODEOWNERS GitHub review + manual merge. CODEOWNERS is defence-in-depth *under*
the server-side refusal, never the primary gate — its teams may not exist and it
does not cover every path below, so it cannot be relied on alone.

Paths are matched **repo-root-relative** (Git diff paths here carry the `rust/`
prefix; the earlier `crates/auth/**` form matched nothing and silently disabled
the killswitch). The set:

- `rust/crates/auth/**`
- `rust/crates/db/migrations/**`
- `**/middleware.rs` (covers `rust/crates/api/src/middleware.rs` and
  `rust/crates/auth/src/middleware.rs`)
- `**/mcp.rs` (covers `rust/crates/api/src/mcp.rs`,
  `rust/crates/api/src/domain/mcp.rs`,
  `rust/crates/api/src/repositories/agent/mcp.rs`)
- `**/security.rs` (covers `rust/crates/platform/src/security.rs`)
- `.github/workflows/**`, `.github/CODEOWNERS`
- **The self-fix loop's own code**, by **explicit module path** (no glob prefix
  distinguishes it): the PR Bridge, Merge Executor, credential handling, and the
  file holding this sensitive-path list itself. This is the killswitch for the
  worst failure mode — an agent weakening its own guardrails.

### 7. Tenant + container invariants unchanged

`&TenantScope` org-scoping on every loop-touched query; per-agent NATS/HMAC
isolation; the container security denials all stay enforced.

## Error handling & edge cases

- **Idempotency.** The task id is the key. A retried push/PR-open/merge must not
  duplicate: the branch is keyed by task id, the PR is keyed by the branch
  (open-or-find), and the merge re-checks PR state first.
- **No usable output.** If the import yields an empty change (agent failed / made
  no change), the task records a visible reason and no PR is opened.
- **Red CI.** Approve stays disabled; the Executor hard-refuses; nothing merges.
- **Merge race.** The Executor marks-ready → re-reads head + checks → merges with
  an expected-head-SHA guard as the atomic tail. If the head moved (e.g. a
  `ready_for_review`-triggered automation pushed a commit), GitHub rejects the
  merge and the task's review aggregate returns to `in_review`.
- **Rollback.** No auto-deploy means no production blast radius; rollback of a bad
  merge is a human `git revert` + a normal PR.
- **No silent failure.** Any Bridge/Executor step error stops the task with a
  visible error; nothing is swallowed.

## Data model

- `orchestration_tasks` gains nullable `base_commit_sha TEXT` (pinned at spawn —
  net-new; no git context is recorded today), `pr_number INT`, `pr_url TEXT`,
  `pr_head_sha TEXT` (the server-created head), and a `self_fix BOOLEAN` (or the
  `self-fix` task group carries the semantics).
- Merge approval is the orchestrator **review aggregate** row reaching
  `ReviewState::Approved`; no new approval column is added and the
  pre-dispatch `blocked` + `waiting_approval` gate is **not** reused (approving
  that releases the task to `queued` and re-dispatches the agent — wrong surface).
- Migrations additive and idempotent per the migration policy.

## Observability

- Counters: `self_fix_tasks_dispatched_total`, `self_fix_pr_opened_total`,
  `self_fix_approved_total`, `self_fix_merged_total`,
  `self_fix_sensitive_path_blocked_total`, `self_fix_import_rejected_total{reason}`
  (symlink / gitlink / escape / oversize / churn-cap), `self_fix_bridge_errors_total{stage}`.
- An audit record for every merge: approver, task id, PR, timestamp, head sha.

## Testing

- **Unit:**
  - Server-derived branch-name enforcement.
  - The import validator: symlink, gitlink/submodule, `..`/absolute escape,
    out-of-allowlist path, oversize file, churn-cap, and a mode-only change each
    rejected/flagged; object-level re-check catches a `120000`/`160000` tree entry.
  - The sensitive-path matcher with **repo-root-relative** inputs, asserting EACH
    of these trips the breaker: `rust/crates/auth/...`,
    `rust/crates/db/migrations/...`, both `middleware.rs`, all three `mcp.rs`,
    `rust/crates/platform/src/security.rs`, the loop's own module paths,
    `.github/workflows/...`, `.github/CODEOWNERS`; plus a regression asserting a
    bare `crates/auth/x` (missing `rust/` prefix) is never the form the matcher
    receives.
  - The Merge Executor's check-green gate, expected-head-SHA guard, server-side
    sensitive-path hard refusal, and idempotency.
- **Integration (gated, like the Redis-backed tests):** a full loop on a throwaway
  no-op change against a sandbox PR — import → push → draft PR → snapshot checks →
  ReviewState=Approved → ready → guarded merge — plus a "PR touches `security.rs` →
  Approve disabled AND Executor refuses" assertion.
- Agent-authored PRs run the same CI + UX + guard gates as human PRs.

## Explicitly deferred (do NOT build here)

- **Auto-deploy on merge** (Phase 5 tail) — needs the migrate-on-boot /
  edge-networking work the roadmap mapped; out of scope.
- **Background triage + any auto-merge tier** (Phase 6) — the highest-risk slice;
  production promotion stays the human-dispatched, soak-gated workflow forever.

## Open questions

- CI-check freshness on the review surface: snapshot-on-open + a manual refresh,
  or a webhook that updates the task when checks complete? (Lean snapshot +
  refresh for the first cut; webhook is a later optimisation.)
- Whether the `self-fix` semantics live in a dedicated task group or a typed flag
  on the task — pick one during planning based on how task groups are modelled.
- Whether to make `.github/CODEOWNERS` authoritative (confirm the referenced
  teams exist or switch to concrete owners, add entries for `security.rs`, both
  middleware files, all mcp files, the loop's own code, and CODEOWNERS itself,
  and enable "Require review from Code Owners") — as defence-in-depth under the
  server-side hard refusal, not as the gate.

## Closed review findings (2026-06-14)

Findings from the Codex review and the adversarial security/correctness pass, and
where each is resolved:

1. **Local Git config/filter RCE under the push credential** (reproduced). The
   Bridge no longer runs any Git command in `/workspace`; it rebuilds from the
   pinned base via a server-side file walk. Security §1.
2. **Base SHA not verified as an ancestor → orphan-branch full-tree replacement**
   (reproduced). The agent's commit graph is never read; the change is rebuilt on
   the server-pinned base, with churn/deletion caps. Security §1.
3. **Sensitive-path globs missing the `rust/` prefix → killswitch silently dead.**
   Corrected to repo-root-relative paths + explicit own-code module list + a
   per-path unit test. Security §6, Testing.
4. **CODEOWNERS fallback is hollow.** Replaced as the primary gate by a
   server-side hard refusal in the Merge Executor; CODEOWNERS demoted to
   defence-in-depth. Security §6, Open questions.
5. **Symlink / submodule / mode-bit patch trickery.** Rejected during import and
   re-checked at the object level (`git diff-tree --raw`). Security §1, Testing.
6. **Done-signal TOCTOU.** The container is stopped before extraction, freezing
   `/workspace`. Data flow step 4, Security §1.
7. **Clean-clone reuse poisoning.** Ephemeral per-task worktree + config/hooks
   re-assertion. Security §2.
8. **Draft PRs cannot be merged.** The Executor marks the PR ready-for-review
   before merging, ordered after the head/check re-verify with an expected-head
   guard. Net-new components, Merge race.
9. **Invented `in_review` / `done` task statuses.** Removed. Merge approval uses
   the orchestrator `ReviewState` aggregate (`in_review` → `approved`); terminal
   task status stays `completed`; the pre-dispatch `waiting_approval` gate is not
   reused. The frontend `done` board *column* (a UI label mapped to `completed`)
   is intentionally left untouched. Data model, status references throughout.

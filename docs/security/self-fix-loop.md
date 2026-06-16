# Self-fix loop security model

The self-fix loop lets an agent propose a code change to this repository as a
GitHub draft pull request that an operator reviews and merges from inside the
app. This note states the trust boundaries and the guarantees — and the
non-guarantees — that follow from them. The operator-facing guide is
[`docs/guides/self-fix-loop.md`](../guides/self-fix-loop.md).

## Trust boundary

- **Agents never hold a GitHub token and never run `git` against the repository.**
  All privileged git (clone, push, merge) runs server-side. The agent only edits
  files in its `/workspace`; the server reads those files as plain data.
- **The server never runs `git` inside an agent `/workspace`.** The PR Bridge
  copies the changed files onto a fresh, clean clone of the default branch in a
  server-owned scratch directory (`SELF_FIX_WORK_DIR`, never under any agent
  mount), validates each file, and rebuilds the change there. A hostile workspace
  cannot smuggle in a `.git` directory, a hook, or a submodule pointer.
- **GitHub credentials live only on the server.** The four `GITHUB_APP_*` values
  configure a GitHub App; the private key is stored encrypted at rest under
  `LLM_ENCRYPTION_KEY`. The server mints a short-lived installation token only at
  the moment it opens or merges a PR. Tokens, the app JWT, request bodies, and
  response bodies are never logged.

## Import validation (PR Bridge)

Before a change can become a PR, each imported file is checked and the change is
re-derived by object hash on the clean clone. The Bridge hard-aborts on:

- **Symlinks and gitlinks** (`120000` / `160000` modes) — no escape via link
  targets or embedded submodules.
- **Path escape** — every path must resolve strictly inside the repository tree.
- **Oversize files and excessive churn** — bounded by import limits.

These run regardless of the change's apparent intent; sensitivity (below) gates
_merge_, but a symlink/gitlink in the import is an unconditional abort.

## Sensitive-path circuit breaker

A pure policy classifies a changed path as **sensitive** when it touches auth,
DB migrations, CI workflows, security/middleware/MCP files, or the self-fix code
itself (its own service, the GitHub App client, this domain policy, and the
self-fix route — own-code is sensitive so the loop cannot weaken its own
guards). A change touching any sensitive path is recorded `sensitive_blocked`.

A `sensitive_blocked` change is **hard-refused from in-platform merge** in
`SelfFixService::approve_and_merge`, before any GitHub call, regardless of what a
client sends. The frontend additionally disables Approve, but that is
defense-in-depth — the server is the boundary. Sensitive changes are left as a
draft PR for a human maintainer to review and merge on GitHub.

## Guarded merge (Merge Executor)

Approve is a dedicated, authenticated, tenant-scoped route — **not** the
pre-dispatch `waiting_approval` button (which would re-run the agent). At merge
time the server independently re-verifies, on the live PR:

- the change is still non-sensitive (re-derived, not trusted from the client);
- CI is green on the PR head;
- the head has **not moved** since review — the merge is **expected-head**, so a
  push between review and approval cannot sneak unreviewed code in.

On any failure nothing merges and the task keeps its prior status. On success the
server squash-merges at the verified head and posts an audit comment naming the
approving operator and recording that no safety check was bypassed. Already-merged
is an idempotent no-op.

## Explicitly out of scope

Auto-dispatch, auto-deploy, and auto-merge are intentionally **not** implemented.
Every merge is a deliberate human action through the Approve route.

## What this does NOT guarantee

- It does not review the _correctness_ of the agent's change — that is the human
  approver's job. The loop guarantees provenance and gating, not that the diff is
  good.
- A maintainer who manually merges a `sensitive_blocked` PR on GitHub is outside
  this loop's controls; the in-platform hard-refusal only governs in-app merges.
- The GitHub App's repository permissions (`contents:write`,
  `pull_requests:write`) are the blast radius if the server is compromised. Scope
  the App to the single target repository and rotate its key on the normal
  schedule.

# Project Git Clone — Design Spec

Status: Draft for review
Date: 2026-06-14
Author: platform
Reviewers: codex (design pass, 10 P1 findings folded)

## 1. Summary

Let a project be created with an optional git repository that the platform
clones into the project's workspace directory, server-orchestrated, with a
visible clone status. The clone runs in an ephemeral, least-privilege,
short-lived container — never as a server-process git call and never lazily on
first agent attach. The git bind is one-shot: the repo is cloned once into an
empty `/workspace/<dir>`; afterward the agent owns the directory and its `.git`
(the server never re-syncs), consistent with the self-fix trust boundary.

This is the long-term, cloud-native shape: an asynchronous attempt lifecycle
driven by the existing Postgres job queue, executed by a disposable container
with a minimal attack surface, with the control plane holding state and never
touching repository content.

## 2. Goals / Non-goals

Goals (v1):
- Optional "Git repository URL" on project create; HTTPS token auth for GitHub
  and GitLab (matches existing credential injection).
- Server-orchestrated clone into the project's workspace dir via an ephemeral
  `agentforge-clone` container.
- One-shot clone of the repository's default branch (full history).
- Durable, recoverable clone-attempt lifecycle with visible status, retry, and
  redacted errors.
- Strict tenant isolation, credential scoping, SSRF egress control, and
  container resource/lifecycle limits.

Non-goals (deferred, recorded for v2):
- Branch / ref / tag selection (v1 records the resolved branch + HEAD SHA only).
- Re-sync / re-clone / pull from the control plane (one-shot bind by design).
- SSH and custom-host credentials; Bitbucket/OAuth provider materialization.
- Git LFS, recursive submodules, sparse/partial checkout.
- Monorepo sub-directory binding.
- User-entered host paths (forbidden — see §6.4).

## 3. Locked decisions

1. Clone runtime: ephemeral, least-privilege, short-lived container.
2. Bind model: one-shot; the agent owns the directory afterward.
3. Clone image: dedicated minimal `agentforge-clone` (not `agent-base`).
4. State of record: a dedicated `project_clone_attempts` table; `projects`
   carries only a denormalized summary for list views.
5. v1 ref scope: default branch only; record resolved branch + HEAD SHA.

## 4. Architecture

```text
Create project (name + optional repo URL + workspace_id)
  -> [single DB transaction]
       validate workspace ∈ org, validate team/permission
       allocate workspace_dir_name (unique per workspace)
       INSERT project (clone_status = queued | none)
       create default project group
       INSERT project_clone_attempts (attempt 1, status = queued)
       INSERT job_outbox (project_clone) -- transactional outbox
     [commit]
  -> outbox publisher moves the row into job_queue (existing pattern)
  -> project_clone worker dequeues (FOR UPDATE SKIP LOCKED)
       mark attempt = cloning, projects.clone_status = cloning, emit event
       create per-clone staging dir under the workspace projects-root parent
       run ephemeral agentforge-clone container:
         - mounts ONLY the per-clone staging dir (not the projects root)
         - restricted egress network
         - host-matched short-lived credential via tmpfs/secret mount
         - git clone <url> into staging; on success writes resolved branch +
           HEAD SHA to a result file; exit code = status
       docker wait + inspect exit; hard timeout; forced cleanup in finally
       on success: same-filesystem atomic rename staging -> /workspace/<dir>
       update attempt + projects.clone_status = ready|failed (redacted error)
       emit WS event + audit event + metrics
  -> agent later attaches to /workspace/<dir>; owns the repo from here on
```

The server (API + worker) never runs `git`, never reads repository content, and
never holds a decrypted credential longer than the moment it is handed to the
container.

## 5. Data model

### 5.1 `projects` (additive)

- `workspace_dir_name TEXT NOT NULL` — the filesystem directory name under the
  workspace projects root. Allocated at create, unique per **workspace** (the
  filesystem boundary), independent of the existing DB `(team_id, slug)`
  uniqueness. Filesystem-safe by policy (see §6.4).
- `clone_status TEXT NOT NULL DEFAULT 'none'` with `CHECK (clone_status IN
  ('none','queued','cloning','ready','failed'))` — denormalized summary of the
  latest attempt, for fast list rendering (mirrors the `runtime_kind` CHECK
  pattern). Source of truth is the attempts table.

`repository_url` already exists. It becomes immutable once an attempt has
reached `queued`/`cloning`/`ready` (see §9).

### 5.2 `project_clone_attempts` (new, source of truth)

One row per clone attempt; supports retry, crash recovery, and diagnosis.

- `id UUID PK`
- `organization_id`, `workspace_id`, `project_id` (tenant scope; FKs)
- `attempt INT NOT NULL` (1-based; unique with project_id)
- `repository_url TEXT NOT NULL` (snapshot at attempt time)
- `provider TEXT` (github|gitlab — resolved by host)
- `credential_id UUID NULL` (which git_credential was selected; never the secret)
- `status TEXT NOT NULL CHECK (status IN
  ('queued','cloning','ready','failed','cancelled'))`
- `resolved_branch TEXT NULL`, `head_sha TEXT NULL` (filled on success)
- `container_id TEXT NULL`, `worker_id TEXT NULL`, `job_id UUID NULL`
- `lease_expires_at TIMESTAMPTZ NULL` (worker lease for crash recovery)
- `error_class TEXT NULL`, `error_message TEXT NULL` (classified + redacted)
- `bytes_cloned BIGINT NULL`, `duration_ms BIGINT NULL` (metrics)
- `started_at`, `finished_at`, `created_at`, `updated_at`
- Unique `(project_id, attempt)`; index `(status, lease_expires_at)` for the
  reconciler sweep.

Migration is additive and idempotent. A schema-contract test pins the new
columns so fresh test DBs and production do not drift.

## 6. Components

### 6.1 Create path (transactional)

A single DB transaction performs: workspace-ownership validation
(`workspace_id ∈ scope.org_id`), team/permission checks (existing
`require_project_creator` / `require_org_manager`), filesystem-safe
`workspace_dir_name` allocation (locked against concurrent same-name creates),
project insert, default-group creation, initial `project_clone_attempts` row,
and an insert into the transactional outbox. **All current create surfaces**
(the flat `ProjectService` and the active legacy-navigation path used by the
settings/sidebar UI via `/teams/:teamId/projects`) go through the same
validated path; the legacy-navigation draft must stop preserving caller-supplied
slugs verbatim and apply the filesystem-safe policy.

If `repository_url` is absent: `clone_status = none`, no attempt, no job.

### 6.2 Transactional outbox + job enqueue

The clone job is enqueued by inserting an outbox row in the create transaction,
then relayed into `job_queue` by the existing outbox publisher
(`orchestration_outbox_publisher` pattern). This removes the "row committed but
job lost" / "job enqueued but project rolled back" race. The job's
`unique_key = project_clone:<project_id>:<attempt>` so a retry creates a new
attempt key and never collides with a genuine second project. A new migration
adds the partial unique index `job_queue (unique_key) WHERE unique_key IS NOT
NULL` that the queue's `ON CONFLICT (unique_key)` already assumes but which does
not yet exist.

### 6.3 `project_clone` worker + reconciler

A dedicated worker (not the generic job handler) owns every status transition:
- On dequeue: set attempt `cloning` + `projects.clone_status = cloning`, take a
  lease (`lease_expires_at`), emit event.
- Run the container (§6.5), `docker wait`, inspect exit, enforce a hard timeout.
- On success: atomic rename (§6.6), write `resolved_branch`/`head_sha`/metrics,
  set `ready`.
- On failure / timeout: redact + classify error, set `failed`, schedule bounded
  retry (new attempt) with backoff.
- Startup + periodic **reconciler sweep**: any `cloning` attempt whose lease has
  expired (worker crashed) is recovered — its container is force-removed and the
  attempt is failed/retried. `pg_notify` is a wake-up only; the sweep is the
  polling fallback so status can never stick at `cloning`/`queued` forever.

### 6.4 Slug / path safety

`workspace_dir_name` is derived from the project name by a filesystem-safe
policy (lowercase, `[a-z0-9-]`, collapse repeats, length cap, reject reserved
names) and made unique per workspace. The clone target is
`projects_root.join(workspace_dir_name)`, canonicalized and asserted to remain
within `projects_root` (defeats traversal). A user-entered host path is never
accepted — the path is always derived, shown read-only in the UI. If a target
directory already exists and is non-empty (e.g. a soft-deleted project's
directory under the same name), creation is refused or the stale directory is
archived first; the allocation is locked in the create transaction.

### 6.5 Ephemeral clone container

Dedicated image `agentforge-clone`: `git`, `ca-certs`, `openssh-client`
(present but unused in v1), `tini`, and `clone-entrypoint.sh`, running as the
same `agent` UID/GID as agent images (so the cloned repo's ownership/safe.directory
is correct for later agents). No Node, Python, docker CLI, sidecar, gh/glab, or
harness — minimal attack surface for untrusted-repo network I/O.

Container contract:
- Inputs: `CLONE_URL`, target staging path, and a host-matched short-lived
  credential delivered via a tmpfs/secret mount (never an env var, never a
  build layer). Optionally configured as a one-shot git credential helper that
  reads the mounted secret.
- Behavior: `git clone <url>` (full history) into the mounted staging dir; on
  success write `resolved_branch` + `HEAD` SHA + byte count to a result file;
  exit 0. On failure exit non-zero with stderr (the worker redacts before
  storing).
- Hardening (reuse `platform/security.rs`): no privileged mode, no host PID, no
  docker socket, resource limits set; **restricted egress network** (cannot
  reach internal services — see §10 SSRF); hard timeout enforced by the worker;
  deterministic name + label `agentforge.project_clone=<attempt_id>` for
  reaping; forced removal in a `finally` and on the startup sweep.

### 6.6 Per-clone staging + atomic rename

The container mounts only a per-clone staging directory (e.g.
`<projects_root>/.clone-staging/<attempt_id>`), **on the same filesystem as the
projects root** so the final `rename(2)` is atomic — it never sees sibling
projects. After the container exits 0, the worker renames staging →
`projects_root/<workspace_dir_name>`. On any failure the staging dir is removed,
so a partial/aborted clone never becomes the live project directory.

### 6.7 Credentials

Resolve exactly one credential matched by the repository URL's host (not "latest
token per provider"). Authorization derives from the creator's project-create
permission in the workspace. The secret is materialized only at container
start, delivered via tmpfs/secret mount, scrubbed from logs and never
serialized (`#[serde(skip_serializing)]`). Whether backend decryption stays
in-process (current behavior) or moves behind a secrets broker is recorded as an
explicit implementation choice; v1 keeps in-process decrypt but hands the
container the token via mount, not env.

### 6.8 Frontend

The active surface is `/api/v1/teams/:teamId/projects` (settings + sidebar), not
the flat `/projects` route — both the route DTOs, `NavProject`/`CreateProjectInput`
types, and the create form change together. The create form gains an optional
"Git repository URL" and shows the derived `/workspace/<dir>` path read-only (no
host-path entry). `ProjectTree` and the project detail show a clone-status badge
(`queued`/`cloning`/`ready`/`failed`) with the redacted error and a retry
action. Status updates arrive over the existing WebSocket realtime channel with
a defined event name + payload + idempotent reducer, plus a list/poll fallback
so a page refresh recovers status if a socket message is missed.

## 7. State machine

```text
none      (no repository_url)
queued -> cloning -> ready
              \-> failed -> (bounded retry) -> queued
cancelled (project deleted mid-flight)
```

Transitions are owned by the `project_clone` worker/reconciler. `projects.clone_status`
mirrors the latest attempt's status.

## 8. Failure, retry, idempotency

- Idempotent by `project_clone:<project_id>:<attempt>`; `ready` is terminal and
  skipped; `failed` retries up to N attempts with backoff, each a new attempt
  row.
- Atomic rename + staging cleanup guarantees no partial project directory.
- Lease + reconciler sweep guarantees no stuck `queued`/`cloning`.
- Container reaping (label + finally + startup sweep) guarantees no orphaned
  credential-holding container.

## 9. Repository URL mutability

`repository_url` is immutable once an attempt has reached `queued`/`cloning`/
`ready` (a one-shot bind cannot be re-pointed by the server). The update path
rejects changes to it in those states; only pre-clone (`none`) projects may set
it. Metadata edits (name/color/description) remain allowed and never trigger a
server-side re-sync.

## 10. Security review

- SSRF: reject non-HTTPS at parse time; the clone container runs on a restricted
  egress network that cannot reach RFC1918 / link-local / metadata / internal
  service addresses, so a crafted or DNS-rebinding repo URL cannot reach
  internal services at git's connect-time DNS resolution. Parse-time host checks
  alone are insufficient and are treated as defense-in-depth only.
- Credential isolation: one host-matched secret, mount-delivered, scrubbed,
  short-lived, never logged/serialized.
- Path traversal: derived + canonicalized + asserted-within-root
  `workspace_dir_name`; no user host paths.
- Tenant isolation: `workspace_id ∈ org` enforced; the container mounts only the
  per-clone staging dir, never the projects root or sibling projects; clones land
  only in the creator's workspace.
- Container hardening: reuse `security.rs` (no privileged / host PID / docker
  socket; resource limits); hard timeout; deterministic reaping.
- Error redaction: classify clone errors and strip URLs / usernames / token
  fragments before persisting `error_message`; raw logs stay server-side.
- Audit + metrics: audit events for project-created-with-repo, clone-started,
  clone-ready, clone-failed, retry-requested, credential-selected-by-host;
  counters/histograms for duration, bytes, status, provider, failure class.

## 11. Resource + abuse limits

Full clone (a development project needs history), but with: a hard wall-clock
timeout, a disk/quota guard before and during clone, LFS skipped by default,
no recursive submodules in v1, and per-workspace concurrency/rate limiting on
clone jobs to bound abuse. Oversized/over-time clones fail with an actionable
error and a documented "clone manually in the agent" escape hatch.

## 12. Observability

Per-attempt structured logs (no secrets), metrics (§10), and a status surface in
the UI. The reconciler exposes a gauge of in-flight/stuck attempts.

## 13. Build sequence (high level; detailed plan follows in writing-plans)

1. Migrations: `projects` additive columns, `project_clone_attempts`,
   `job_queue` partial unique index, schema-contract test.
2. Domain + repo: attempt aggregate, filesystem-safe slug/path policy applied to
   all create surfaces, workspace-ownership validation.
3. Transactional create + outbox enqueue across flat + legacy-navigation paths.
4. `agentforge-clone` image + `clone-entrypoint.sh` + factored credential/git
   config logic; platform runtime support for the restricted network, staging
   mount, labels, timeout, reaping, exit inspection.
5. `project_clone` worker + reconciler + retry/backoff + redaction + metrics +
   audit.
6. API: create accepts repo URL; status projection; retry endpoint; immutability
   rule.
7. Frontend: create-form field + read-only path, status badge + retry, WS event
   + reducer + poll fallback, type/DTO sync.
8. Tests: unit (policy, state machine, redaction, path assertion), integration
   (`sqlx::test` create→attempt→status, reconciler recovery), security
   (traversal, SSRF egress, tenant boundary, no-secret-in-logs), and an
   end-to-end clone against a controlled repo.

## 14. Open implementation choices (recorded, decided at plan time)

- Secrets broker vs in-process decrypt (v1: in-process decrypt, mount delivery).
- Exact restricted-network mechanism (dedicated Docker network with egress
  filtering vs proxy) — chosen against the deployment's networking in the plan.
- Outbox table reuse vs a clone-specific outbox.

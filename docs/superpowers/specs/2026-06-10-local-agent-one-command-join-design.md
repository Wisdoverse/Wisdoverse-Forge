# One-Command Local Agent Join + Staging Bug Fixes

Date: 2026-06-10
Status: approved for implementation (autonomous session; review happens on the PRs)

## Why

Operator feedback from staging dogfooding (2026-06-10):

1. "Connect a Local Agent" requires copying a multi-line env-export block and
   assumes `agentforge-sidecar` is already installed. Competing products
   (e.g. ByteDance Coze) connect a local bridge with a single command in the
   shape `npx coze-bridge@latest --pat-token=... --pair-code=...`. We need the
   same one-command experience, including a pairing code.
2. Creating any Container CLI agent returns HTTP 500 on staging.
3. The `/admin` page crashes to the error boundary.
4. The Getting Started ("Start") checklist progress bar cannot reach 100% and
   never hides.

## Findings (verified on staging + current `main`)

- **Create-agent 500**: `AgentRepository::create`
  (`rust/crates/api/src/repositories/agent/mod.rs:242`) inserts without
  `runtime_kind`. Migration 062 set `DEFAULT 'api'`; 063 added
  `agents_runtime_kind_invariants`. A row with `cli_tool` set and
  `runtime_kind='api'` violates the CHECK → 500 for every Container CLI
  create. Server log confirms:
  `new row for relation "agents" violates check constraint "agents_runtime_kind_invariants"`.
  The aggregate path (`create_aggregate_in_tx`) is correct; only the legacy
  path regressed. Latent sibling: `McpAgentRepository::insert_agent` binds
  `container_id` but has no `cli_tool` column, so with `container_id` set NO
  constraint arm can match.
- **Admin crash**: `admin.store.ts` `loadUsers` parses
  `{ users, total, page }` but the backend returns `{ ok, data }` →
  `users` becomes `undefined` → `users.length` throws in `UserManagement`.
  Additional phantom contracts: `PUT/DELETE /api/v1/admin/users/{id}` and
  `GET /api/v1/admin/orgs` do not exist on the backend
  (`routes/admin.rs` has only `GET /admin/users`, `GET /admin/organizations`);
  `loadHealth` calls `/api/v1/health` (real route is `/api/health`) and
  expects `checks.<svc>.status` objects while the API returns booleans.
  Backend admin user rows serialize snake_case (`display_name`, `is_admin`)
  while the UI type expects camelCase plus fields that do not exist
  (`role`, `status`, `sessionsCount`).
- **Local enroll blocked on staging**: `POST /api/v1/agents/local-enroll`
  returns `errors.agent.enroll.plaintext_nats_blocked` because
  `NATS_AGENT_URL` is unset/plaintext and `ALLOW_PLAINTEXT_HOST_NATS` is not
  set. Deployment configuration, not code.
- **Progress bar**: the Start page is a real-state checklist (8 steps). It
  stalled because step 3/4 depend on a working create/enroll flow (bugs
  above). Separately, at 100% the bar stays visible by design today.

## Design

### A. One-command local agent join (feature)

Command format follows the Coze reference (single command, short flags,
pairing code), but is served from the operator's own deployment so it works
self-hosted and air-gapped, with no Node prerequisite:

macOS / Linux:

```bash
curl -fsSL https://forge.example.com/api/v1/agents/local-join/script | sh -s -- --code afj_<pair-code>
```

Windows (PowerShell):

```powershell
$env:AGENTFORGE_JOIN_CODE='afj_<pair-code>'; irm https://forge.example.com/api/v1/agents/local-join/script.ps1 | iex
```

Both commands are returned by the enrollment API and rendered in the UI with
copy buttons; the pair code is embedded so the operator pastes exactly one
line.

**Join code ("pairing code")**

- Format `afj_` + 43 base64url chars (32 random bytes). Prefix supports
  secret scanning; 256-bit entropy makes guessing infeasible.
- Stored as SHA-256 hash in new table `agent_join_codes`
  (id, organization_id, agent_id FK CASCADE, code_hash UNIQUE, expires_at,
  used_at, claim_count, created_at). Plaintext never persisted.
- TTL 15 minutes (domain constant). Codes may be claimed multiple times until
  expiry: a script crash after first claim must not strand the operator, and
  the idempotent-replay enrollment path can reuse the same UX. `used_at` +
  `claim_count` recorded for audit.
- Minted inside the enrollment transaction (cold path) and freshly on
  idempotent replay.

**Endpoints** (public infrastructure, like `/health` — the code IS the
credential; mounted outside auth middleware):

- `GET /api/v1/agents/local-join/script` → POSIX sh bootstrap
  (text/x-shellscript). Server URL + binary base URL rendered in; contains no
  secrets; `Cache-Control: no-store`.
- `GET /api/v1/agents/local-join/script.ps1` → PowerShell variant.
- `POST /api/v1/agents/local-join/claim` body `{"code": "..."}`,
  optional `"format": "json" | "exports" | "psexports"`.
  Valid code → enrollment env (JSON map, or ready-to-eval export lines that
  reuse the existing quoting policy). Unknown/expired → single opaque 404
  (`errors.agent.join.invalid_code`) — no oracle between unknown and expired.
  `Cache-Control: no-store` + `Pragma: no-cache`.

**Bootstrap script behavior** (sh; PowerShell mirrors):

1. Parse `--code` (or `AGENTFORGE_JOIN_CODE`), optional `--cwd`.
2. Locate `agentforge-sidecar`: PATH → `~/.agentforge/bin/` → download
   `agentforge-sidecar-<os>-<arch>[.exe]` from the binary base URL
   (default: this repo's GitHub latest release; override via
   `HOST_JOIN_BINARY_BASE_URL` for mirrors/air-gap), chmod +x, print a
   cosign-verification hint (runbook covers full verification).
3. Claim the code (`format=exports`), write env to
   `~/.agentforge/agents/<agent-id>.env` (chmod 600) — avoids secrets in
   shell history beyond the short-lived pair code itself.
4. Warn (not fail) if the selected Container CLI (`claude`/`codex`/...) is not
   on PATH.
5. `exec agentforge-sidecar` in the foreground with a friendly "leave this
   window open" banner.

**Backend layering (DDD)**

- `domain/agent.rs`: `JoinCode` (generate/parse/hash), TTL constant,
  `HostAgentEnrollmentPolicy::{join_command, join_command_powershell,
shell_export_lines, powershell_export_lines}`, typed error helpers.
- `repositories/agent/join_code.rs` (new aggregate submodule):
  `store_in_tx`, `find_claim_by_code_hash` (single JOIN to agents, validity
  window in SQL), `record_claim`. The claim lookup is intentionally
  scope-less (the code authenticates), same category as login-by-email; the
  join row pins org + agent.
- `services/agent_enrollment.rs`: mint in enroll/replay; new `claim()`
  returning the same env the original enrollment produced (container/api
  agents rejected via the existing `EnrolledHostCli` typestate).
- `routes/agent_join.rs`: the three public handlers + script templates.
- Response additions (camelCase): `enrollment.joinCode`,
  `enrollment.joinCodeExpiresAt`, `enrollment.joinCommand`,
  `enrollment.joinCommandPowershell` (omitted when `APP_URL` unset).
- Config: `HOST_JOIN_BINARY_BASE_URL` (optional; default GitHub releases URL
  of this public repo).

**Frontend (FSD)**

- `entities/agent` API types extended with the new enrollment fields.
- `CreateAgentModal` success view leads with the one-line join command
  (OS toggle Bash/PowerShell + copy + expiry note); the existing env block
  moves into a collapsed "Manual setup (advanced)" section.
- `AgentListView` HostCliEnrollmentPanel copy updated to point at the
  one-command path.

**Platform CLI**

- `agentforge agents enroll-local` prints the join command first when the
  response carries it (existing env-block output kept for `--shell`).

**Docs**

- `docs/runbooks/host-cli-agent-enrollment.md`: new "One-command join
  (recommended)" fast path at the top; manual flow demoted to advanced.
- `docs/guides/configuration.md`: `HOST_JOIN_BINARY_BASE_URL`,
  `ALLOW_PLAINTEXT_HOST_NATS` cross-reference.
- `docs/architecture/glossary.md`: "Join code".

**Security analysis**

- Pair code in the command line ≈ Coze's model, but ours is 15-minute,
  audit-logged, hash-at-rest, and exchanges into per-agent scoped NATS/HMAC
  credentials — strictly better than the current UX of pasting the raw
  long-lived credentials themselves.
- Claim responses are `no-store`; script GETs carry no secrets.
- Public endpoints validate nothing but the code: constant-shape 404,
  hash-keyed lookup, no enumeration surface. Rate limiting piggybacks on the
  existing tower stack if present; 256-bit entropy is the primary defense.
- TLS gate behavior unchanged: enrollment still refuses plaintext NATS unless
  explicitly allowed.

### B. P0: runtime_kind on every agents INSERT (bug)

- Domain: `RuntimeKind::from_legacy_create(cli_tool: Option<&str>)` →
  `container` iff `Some`, else `api` (host CLI never uses this path).
- `CreateAgentParams` carries the derived kind; `AgentRepository::create`
  binds it explicitly.
- MCP path: `McpAgentRecord`/`McpAgentInsertRecord` gain `cli_tool`;
  kind derived `container` iff `container_id.is_some()` (requires cli_tool —
  callers must supply; docker runtime knows its tool) else `api`.
- Regression tests (`#[sqlx::test]`): container create succeeds with
  `runtime_kind='container'`; provider create → `'api'`; MCP insert with
  container id satisfies the CHECK.

### C. Admin page truthful fix (bug)

Frontend-only contract alignment (backend admin API is the source of truth):

- `loadUsers`: parse `{ ok, data }`, map snake_case rows to the UI type,
  derive `role` from `is_admin`, `status` from `deleted_at`,
  drop `sessionsCount` column; client-side search filter; limit/offset
  pagination with "next page iff page full".
- Remove the role editor + delete actions that call endpoints which have
  never existed; render access level read-only (backend role management is a
  separate backlog item).
- `loadOrgs`: call `/api/v1/admin/organizations`; map fields; counts the
  backend doesn't provide render as em-dash.
- `loadHealth`: call `/api/health`; map boolean checks to up/down statuses.

### D. Getting Started polish (bug-adjacent)

At 100% (`setupComplete`) hide the progress bar block and show the completed
banner state that already exists (`readyTitle`). No checklist semantics
change.

### E. Staging deployment notes (not a code change)

- Rebuild + redeploy server image (carries B + A), rebuild frontend bundle to
  the webroot (carries A + C + D).
- For local agents to connect to staging, `NATS_AGENT_URL` must point at a
  NATS address reachable from operator machines, TLS-fronted, or
  `ALLOW_PLAINTEXT_HOST_NATS=true` must be set consciously (dev/test only).
  This is an infra decision to confirm with the operator.

## Delivery plan

1. PR-1 (P0): B — small, surgical, immediate staging deploy.
2. PR-2 (feature): A — join codes end to end + docs.
3. PR-3 (fix): C — admin page.
4. PR-4 (polish): D.

Validation per CLAUDE.md blast radius: narrow Rust tests then
`cd rust && make ci` for PR-1/2; `npm run fsd:check && lint && format:check
&& typecheck` + Vitest for PR-2/3/4; staging smoke after deploy.

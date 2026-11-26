# ADR 0004 — TenantScope guards tenant-scoped queries

## Status

Accepted.

## Context

This is a multi-tenant system. Every authenticated query must be scoped to an
organization so that data from one tenant never leaks to another. Doing this
through ad-hoc `WHERE organization_id = ?` calls is fragile: a single missing
clause silently broadens a query across all tenants. The codebase needs a
type-level guard that makes the wrong query hard to write.

## Decision

`agentforge_core::TenantScope` is the only way to express "the caller's
authorization context." It carries `OrgId`, `UserId` and optional
workspace/team/project axes. Its fields are private.

Rules:

1. **Who may construct a scope.** `core` cannot limit visibility to other
   crates, so the constructors are `pub` and a guard enforces who calls them:
   - `TenantScope::with_axes` — only the auth middleware
     (`rust/crates/auth/src/middleware.rs`, `from_request_parts`) and the
     WebSocket gateway (`rust/crates/api/src/gateway/ws.rs`, `ws_handler`),
     each after validating the JWT the values come from.
   - `TenantScope::for_background(org_id, acting_user, workspace_id)` —
     background work (workers, schedulers, sweeps). The organization comes from
     a database row or server-enqueued job the worker loaded, never from
     request input. The acting user is the stored owner of the work (for
     example a recurring task's `created_by`, which is persisted and has a
     foreign key to `users`), or `UserId::SYSTEM` (the nil UUID) when the work
     has no owner. Current callers: the self-fix PR worker (system user), the
     recurring-task runner (`created_by`), the compliance exporter (a live
     member) and the Agent reconcile worker (the Agent's owner and workspace).
   - `TenantScope::new` — test code only.
2. Every tenant-scoped repository method takes `&TenantScope`, or a
   `ScopedRead`/`ScopedWrite` proof built from it, as its first business
   argument and applies the organization filter **inside the SQL**.
3. **By-id writes carry the organization in the statement.** An `UPDATE`,
   `DELETE` or upsert conflict branch that targets a row by id constrains
   `organization_id = $n`, or an `EXISTS` through the owning row when the table
   has no such column (for example `agent_collaborators` through `agents`). A
   check the caller made earlier does not replace it. When the predicate
   excludes the row, the method returns exactly what it returns for an id that
   does not exist (the same error, `Ok(None)` or `Ok(false)`), so nothing tells
   the caller that the id exists in another organization.
4. Routes obtain the scope from the `AuthUser` extractor and pass it through
   the service layer to the repository. Services may narrow but never widen
   a scope.
5. **Pre-auth queries** — operations that must run _before_ a scope exists
   (login by email, context-switch authorization for a target org) take raw
   `Uuid`s. Every such method carries a doc comment that explains why it
   bypasses `TenantScope`: `repositories::user::UserRepository`,
   `repositories::identity::OrganizationRepository::find_member_role`,
   `repositories::identity::TeamRepository::is_user_member`,
   `repositories::workspace::WorkspaceRepository::exists_in_org`, and
   `repositories::project::ProjectRepository::user_can_read`.

### Enforcement

`scripts/check-scope-construction.sh`, run by
`rust/crates/auth/tests/scope_construction_allowlist_test.rs`, scans every
production line under `rust/` and rejects `TenantScope::new`,
`TenantScope::with_axes` and `ScopedRead`/`ScopedWrite::unchecked_*` outside
the allowlist in `rust/crates/auth/src/scope_construction_allowlist.txt`.
Entries name one function (`path::fn_name`) or, for the definition and the
test-only helper, a whole file. The scanner also rejects an aliased import or
type alias of `TenantScope`, whitespace or a line break inside the constructor
path, a function pointer to a constructor, and any `impl` block for
`TenantScope` outside its definition. It is a text scan: a macro that pastes
the identifier together is left to review.

Cross-organization tests for rule 3 live in
`rust/crates/api/tests/tenant_predicate_writes_test.rs` and
`repositories/agent/mcp.rs`.

### Legitimately unscoped writes

These repository writes do not take a request scope. Each is listed with the
reason it is safe:

- **MCP bridge** (`repositories::agent::McpAgentRepository`): the internal MCP
  bridge has no `TenantScope`; its lease and delete writes take the caller's
  organization id as a raw `Uuid` and constrain by it in SQL.
- **Pre-authentication:** `credential::api_key` `update_last_used` (id from the
  key-hash lookup); `agent::join_code` `record_claim` (pairing-code hash);
  `resource::invite` `grant_memberships` and `mark_accepted` (invite-token
  hash); `user` `update_last_login`, `update_password_hash` and the
  password-reset token writes.
- **Platform administrator, deployment-wide:** `agent`
  `delete_as_platform_admin_in_tx` (checks the expected container id);
  `admin` `set_user_admin`, `soft_delete_user`; SCIM and identity-provider
  deprovisioning in `user` (`deactivate_user`, `invalidate_sessions`,
  `add_team_membership`, `remove_team_membership`).
- **Keyed by the caller's own user id:** `credential::cli` and
  `user::llm_config` writes, `user` `merge_preferences` — these tables hold
  per-user rows without an organization.
- **Background workers on rows they loaded:** `project_clone` clone-attempt
  bookkeeping (claim, credential and container ids, publish, finalize, lease,
  recovery); `recurring_task` `claim_due`; `credential::cli` refresh-sweep
  counters.
- **Cross-tenant maintenance:** `retention` purges, `usage_analytics`
  refresh bookkeeping, the startup legacy-hash reset.
- **Provider webhooks:** `billing` Stripe subscription and invoice upserts,
  keyed by Stripe ids from signed payloads.

WebSocket and MCP gateways authenticate before reaching application code; the
WebSocket gateway builds its scope from the validated JWT as described in rule 1.

## Consequences

- Cross-tenant data leaks require active circumvention, not omission. A new
  repository method without `&TenantScope` is an immediate review red flag.
- A by-id write stays inside the organization even when its caller forgot its
  own check.
- Tests that exercise repository methods need a real or test `TenantScope`,
  which forces them to think about the org they are simulating.
- Pre-auth queries are visible in code review because they have to declare
  the exception. Hidden untrust-by-default queries are not.
- Background jobs and Temporal workflow activities that act on behalf of a
  tenant build their scope with `TenantScope::for_background` from a row they
  loaded; they cannot take an organization from request input.

## References

- `rust/crates/core/src/tenant.rs` — `TenantScope` definition.
- `rust/crates/auth/src/middleware.rs` — the request-scope constructor.
- `scripts/check-scope-construction.sh` — the construction guard.
- [Backend API and data rules](../agents/backend.md#api-and-data).
- PR #289 — the most recent precedent for documenting a pre-auth exception.

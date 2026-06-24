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

`agentforge_core::TenantScope` is the only way to express "the authenticated
caller's authorization context." It carries `OrgId`, `UserId`, optional
workspace/team/project axes, and the caller's role.

Rules:

1. `TenantScope` is constructed **only** by the auth middleware
   (`rust/crates/auth/src/middleware.rs`). Application code cannot construct
   one. This makes "did we authenticate first?" a type-system question.
2. Every tenant-scoped repository method takes `&TenantScope` as its first
   business argument (after `&self`) and applies the org filter inside the
   SQL.
3. Routes obtain the scope from the `AuthUser` extractor and pass it through
   the service layer to the repository. Services may narrow but never widen
   a scope.
4. **Pre-auth queries** — operations that must run _before_ a scope exists
   (login by email, context-switch authorization for a target org) take raw
   `Uuid`s. Every such method carries a doc comment that explains why it
   bypasses `TenantScope`. There are currently five such methods, all in
   `repositories::user::UserRepository`,
   `repositories::identity::OrganizationRepository::find_member_role`,
   `repositories::identity::TeamRepository::is_user_member`,
   `repositories::workspace::WorkspaceRepository::exists_in_org`, and
   `repositories::project::ProjectRepository::user_can_read`.

WebSocket and MCP gateways construct their own scope from validated JWT
claims through the same middleware path before reaching application code.

## Consequences

- Cross-tenant data leaks require active circumvention, not omission. A new
  repository method without `&TenantScope` is an immediate review red flag.
- Tests that exercise repository methods need a real or test `TenantScope`,
  which forces them to think about the org they are simulating.
- Pre-auth queries are visible in code review because they have to declare
  the exception. Hidden untrust-by-default queries are not.
- Background jobs and Temporal workflow activities that act on behalf of a
  tenant must thread a `TenantScope` through their inputs; they cannot
  invent one mid-task.

## References

- `rust/crates/core/src/tenant.rs` — `TenantScope` definition.
- `rust/crates/auth/src/middleware.rs` — the sole constructor.
- `AGENTS.md` — "Backend Contracts" / "Tenant-scoped repository methods".
- PR #289 — the most recent precedent for documenting a pre-auth exception.

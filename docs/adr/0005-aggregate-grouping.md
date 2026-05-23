# ADR 0005 — Repository aggregate grouping convention

## Status

Accepted.

## Context

Some bounded contexts span multiple tables that change together: the identity
context owns `organizations`, `teams`, and `groups`; the credential context
owns OAuth state, encrypted secrets, and per-provider configuration. Pushing
all of these into a single `repositories/identity.rs` file produces 2000-line
modules; splitting them into top-level peers (`identity_team.rs`,
`identity_group.rs`) loses the grouping.

## Decision

Repositories are organized by aggregate, where an _aggregate_ is the smallest
cluster of tables a service treats as a unit:

- **Multi-table aggregates** live under `repositories/<aggregate>/` with one
  submodule per root concept and a `mod.rs` that re-exports the repository
  types:
  - `repositories/agent/`
  - `repositories/context_candidate/`
  - `repositories/credential/`
  - `repositories/identity/` — organization, team, group
  - `repositories/orchestration/`
  - `repositories/resource/`
  - `repositories/skill/`
  - `repositories/user/`
- **Single-table aggregates** stay as flat files at the top level
  (`repositories/workspace.rs`, `repositories/project.rs`, etc.).

When a new table joins an existing aggregate, add it as a submodule and
re-export from the aggregate `mod.rs`. Do not create a new top-level
repository file unless the table genuinely owns its own boundary.

Services compose multiple repositories when they need to cross aggregate
boundaries (e.g. `AuthService` uses `OrganizationRepository`, `TeamRepository`,
`WorkspaceRepository`, and `ProjectRepository` to authorize a session context
switch). Services never reach into another aggregate's internal tables
directly.

## Consequences

- Code review can focus on a single aggregate without scanning unrelated
  tables.
- Adding a new column or constraint to an aggregate stays local; the diff
  touches one directory.
- Migration order is implicit: aggregates that own a table own its
  migrations.
- The number of top-level modules is bounded by the number of bounded
  contexts, not by the number of tables. This keeps `repositories/mod.rs`
  scannable.
- Cross-aggregate orchestration is forced into the service layer, which is
  where it belongs.

## References

- `rust/crates/api/src/repositories/` — current aggregate directory layout.
- `docs/architecture/aggregate-catalog.md` — table of aggregates and their
  modules.
- PRs #211–#217 — the eight-aggregate consolidation series.

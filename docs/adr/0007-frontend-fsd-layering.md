# ADR 0007 — Frontend Feature-Sliced Design boundaries

## Status

Accepted.

## Context

The React/Vite frontend grew through ad-hoc directories (`components/`,
`pages/`, `utils/`) until cross-cutting imports made feature isolation
impossible: changing a "session" component reached into the activity feed, the
nav, and the settings drawer simultaneously. Feature-Sliced Design (FSD) is the
explicit answer: a layered topology with a one-way import graph.

## Decision

The active React app lives at `src/app` and follows strict FSD:

```text
app -> pages -> widgets -> features -> entities -> shared
```

Imports may only point downward through this layer order. The constraints are:

- A **feature** may import its own files plus `entities` and `shared`.
- Cross-feature imports are forbidden. Behavior that two features both need
  is promoted to `widgets`, `entities`, or `shared`.
- `entities` owns the domain API/types and domain-specific stores.
- `shared` owns generic utilities, the global UI kit, the API client primitives,
  and cross-slice context providers.
- Routes, layouts, and providers belong only under `src/app/routes`,
  `src/app/pages`, and `src/app/app`.

No active frontend code lives outside `src/app`. When behavior is needed from
a retired root-level path, move or adapt it under the right FSD layer first.

`npm run fsd:check` is wired into `npm run lint` so CI rejects boundary
regressions.

## Consequences

- Features become deletable. Removing one is a `rm -rf` of its directory
  plus a route table edit; no other feature depends on its internals.
- Onboarding gets a single, navigable model. A new contributor knows where a
  component lives from its name.
- Architectural discipline is enforced by tooling, not just convention.
- WebSocket dispatch and shared realtime state live in `src/app/hooks` and
  the owning `features/*/model` slice respectively, not scattered.
- The cost is one more boundary to think about when adding small features.
  The team accepts that cost as a one-time mental tax in exchange for a
  scalable structure.

## References

- `src/app/` — current layout.
- `npm run fsd:check` — boundary linter.
- `AGENTS.md` — "Frontend Contracts" section.
- Feature-Sliced Design upstream docs.

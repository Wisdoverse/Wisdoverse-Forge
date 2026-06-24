# Versioning and Release Policy

This document describes how Wisdoverse Forge versions its public surfaces and
how breaking changes are introduced. The policy applies to the API contract,
the SPEC document, the frontend application, and the published artifacts. It
does not apply to the internal Rust crate versions, which are private
implementation details.

## API Versioning

The HTTP and WebSocket API live under `/api/v1`. The `v1` prefix is a stable
contract: a client that worked yesterday should continue working today.

### Backward-Compatible Changes

The following are not breaking and do not require a version bump:

- Adding a new endpoint under `/api/v1/`.
- Adding a new optional field to a request body. Missing fields keep the
  prior default.
- Adding a new field to a response body. Clients that ignore unknown fields
  continue working.
- Widening an enum's accepted values, when the new value can be returned only
  after a feature flag is enabled.
- Tightening an internal implementation (faster query, better caching) when
  the externally observable behavior does not change.

### Breaking Changes

The following are breaking and require either:

1. A new endpoint at `/api/v1/...` that preserves the old behavior, with the
   old endpoint deprecated and removed only after the deprecation window
   below; or
2. A new prefix `/api/v2/...` for the full surface, with `/api/v1` continuing
   to serve the prior contract during the deprecation window.

Breaking changes include:

- Removing or renaming an endpoint, query parameter, request field, response
  field, or path segment.
- Narrowing an enum (rejecting a value that used to be accepted).
- Changing the semantics of an existing field (units, encoding, time zone).
- Changing the status code or error code returned for a given input.
- Removing or renaming a header the API used to set (e.g.
  `Set-Cookie: af_rt`).

### Deprecation Window

A deprecated surface stays callable for **90 days** after its replacement
lands. During the window:

- The OpenAPI spec marks the deprecated operation as `deprecated: true`.
- Responses include a `Deprecation` header per RFC 9745 (or the closest
  available form) with a date.
- The CHANGELOG entry that introduced the deprecation lists the removal
  release.

At the end of the window the deprecated surface is removed. There is no
indefinite legacy mode.

## CHANGELOG Discipline

Every PR that ships a user-visible change updates `CHANGELOG.md` under
`## Unreleased`. Entries are categorized:

- **Added** — new endpoint, new field, new feature flag.
- **Changed** — backward-compatible change (e.g. new optional field on an
  existing endpoint).
- **Deprecated** — surface marked for removal, with target window.
- **Removed** — surface deleted after its deprecation window.
- **Fixed** — bug fix that does not change the contract.
- **Security** — security fix; reference the relevant advisory if public.

Release cuts move `## Unreleased` to a dated heading. The CHANGELOG is the
source of truth for what shipped between two artifacts.

## Internal Rust Crate Versions

The Rust workspace's internal crate versions (`agentforge-core`,
`agentforge-api`, `agentforge-orchestrator`, etc.) are not a public contract.
They follow semver only insofar as workspace-relative imports require an
exact match; they are never published to crates.io and are not consumed
outside this repository.

External operators integrating with Wisdoverse Forge consume the HTTP API
under `/api/v1`, not the Rust types.

## Frontend Versioning

The React/Vite frontend is a rolling deploy with no public version number.
Operators always run the build that pairs with their backend release. The
frontend may add features ahead of public release behind a feature flag
served by `/api/v1/feature_flags`; deactivating the flag is the rollback.

## Architecturally Significant Changes

Any change that warrants an ADR (see [docs/adr/README.md](adr/README.md))
records the version it landed under and links to the CHANGELOG entry. ADRs
that supersede a prior decision update the prior ADR's status line to
`Superseded by ADR-NNNN`.

## Release Cadence

Releases are produced from `main` on demand. There is no fixed cadence in
this repository; the operator decides when to deploy from upstream. Each
release tag points at a `main` commit whose CI was green.

CI must pass before a release tag is cut. The full CI matrix (lint, format,
typecheck, fsd:check, frontend tests, Rust tests, clippy, audit, Trivy,
secret scan, code scanning) is the gate.

# ADR 0003 — Three-layer error handling in Rust

## Status

Accepted.

## Context

A Rust service handles three kinds of error context: typed domain errors
("validation failed, password too short"), opaque infrastructure errors ("DB
connection dropped"), and HTTP responses ("422 with this JSON body"). Mixing
them produces noisy handlers (`anyhow` everywhere), leaks internal detail to
clients, or burns invariants into the type system that nobody reads.

## Decision

Three layers, one library per layer:

1. **Domain errors — `thiserror`.** `agentforge_core::ErrorKind` enumerates the
   public failure modes the API can express: `Unauthorized`, `Forbidden`,
   `Validation(String)`, `Unprocessable(String)`, `Conflict(String)`,
   `NotFound(String)`, `Unavailable(String)`, `Internal(anyhow::Error)`.
   Domain code returns `Result<T, AppError>` where `AppError` wraps an
   `ErrorKind`. Domain code never converts errors to HTTP responses.
2. **Infrastructure context — `anyhow`.** Inside `Internal`, infrastructure
   errors carry an `anyhow::Error` so callers can attach `.context("while
reaching nats")` without inventing a typed variant per cause. The
   public-facing variant stays `Internal`; the context is recorded in logs.
3. **HTTP mapping — `AppError::IntoResponse`.** Routes return `Response` or
   `AppResult<Json<…>>` and let the `IntoResponse` impl pick the status code
   and JSON body. Handlers never construct error status codes manually.
   Internal errors map to `500` and log the `anyhow` chain; the body never
   reveals it.

`clippy::unwrap_used` is denied in handler code. Use `?` and let the conversion
chain take over.

## Consequences

- Public error shape is stable. Adding a new client-facing error means
  extending `ErrorKind`, which forces an explicit code review.
- Infrastructure failures get full diagnostic chains in logs without polluting
  the public error contract.
- Routes shrink: a handler that orchestrates a few service calls usually only
  uses `?` and never matches on error kinds.
- Adding a new HTTP status requires editing one place (`IntoResponse`) instead
  of every route that might need it. This made it possible to revisit (e.g.
  give `Forbidden` a message payload) without rewriting handlers.
- Services that need to map repository errors to richer messages (e.g.
  `users_email_key` unique constraint -> `Conflict("email already registered")`)
  do so once at the repository boundary; routes see the typed result.

## References

- `rust/crates/core/src/error.rs` — `AppError`, `ErrorKind`, `IntoResponse`.
- `AGENTS.md` — "Backend Contracts" / "Rust error handling follows the
  3-layer pattern".
- `thiserror`, `anyhow`, and Axum `IntoResponse` upstream documentation.

# Dependency Security Policy

## Scanning

CI runs on every MR:

- `npm audit` — npm dependencies
- `trivy fs` — all filesystem dependencies
- `cargo audit` — Rust crate dependencies (advisory DB via RustSec)

Renovate auto-creates MRs for updates (see Issue #1 Dependency Dashboard).

## Severity SLA

| Severity | Pipeline | Fix deadline     | Action                              |
| -------- | -------- | ---------------- | ----------------------------------- |
| CRITICAL | Blocks   | 24 hours         | Patch immediately, hotfix if needed |
| HIGH     | Blocks   | 7 days           | Prioritize in current sprint        |
| MEDIUM   | Warns    | 30 days          | Schedule in backlog                 |
| LOW      | Warns    | Next maintenance | Bundle with other updates           |

## Update Procedures

- `npm audit fix` for npm vulnerabilities (verify with `npm audit --json`)
- `cargo update -p <crate>` for Rust deps (verify with `cd rust && cargo audit`)
- Always run full test suite (`npm run test:unit` and `cd rust && cargo test --workspace`) after dependency updates
- Renovate MRs: review changelog, merge if CI passes, close if superseded by manual fix

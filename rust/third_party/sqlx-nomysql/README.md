# sqlx-nomysql — vendored PostgreSQL-only fork of `sqlx`

Upstream: https://github.com/launchbadge/sqlx — crate `sqlx` v0.8.6.

## Why this fork exists

The `sqlx` crate declares `sqlx-mysql` and `sqlx-sqlite` as `optional = true`
dependencies. Cargo records disabled weak optional dependencies in `Cargo.lock`
(see rust-lang/cargo#10801), which means these drivers appear in the lockfile
even when only the `postgres` feature is enabled.

`sqlx-mysql` pulls in `rsa 0.9.10` transitively, which carries an unfixed
advisory — **RUSTSEC-2023-0071** ("Marvin Attack" timing sidechannel). The
`rsa` crate has no upstream fix available: RustCrypto's constant-time
replacement is still in development.

`cargo audit` reads `Cargo.lock` unconditionally, so the `rsa` advisory shows
up on every CI run even though:

- We only enable the `postgres` feature.
- `rsa` is not linked into any built binary (verified via `nm` and
  `cargo tree --workspace --all-features --target all`).
- The vulnerable code path is only reachable through a MySQL connection,
  which this codebase never opens.

Rather than silencing the advisory in a `cargo-audit` ignore list, this fork
physically removes the `sqlx-mysql` and `sqlx-sqlite` optional deps from
`sqlx`'s `Cargo.toml`. Both drivers, along with `rsa`, disappear from
`Cargo.lock` entirely.

The matching fork of `sqlx-macros-core` lives at
`../sqlx-macros-core-nomysql/`.

## Re-syncing with upstream

When bumping `sqlx` to a newer version:

1. Download the new crate tarball:
   ```bash
   curl -sSL "https://crates.io/api/v1/crates/sqlx/<NEW>/download" \
     | tar xz -C /tmp/
   ```
2. Copy the fresh `src/` into this directory, overwriting.
3. Re-apply the `Cargo.toml` delta (see the current file):
   - Remove `[dependencies.sqlx-mysql]` and `[dependencies.sqlx-sqlite]`.
   - Remove `mysql`, `sqlite`, `sqlite-unbundled`, `sqlite-preupdate-hook`, and
     `all-databases` features.
   - Remove every `sqlx-mysql?/*` and `sqlx-sqlite?/*` / `sqlx-sqlite/*`
     entry from other feature lists.
   - Drop the `[[test]]` / `[[bench]]` / `[dev-dependencies.*]` / docs.rs
     metadata sections — they are not needed in a consumed-only vendor.
4. Also bump the version pin in `../sqlx-macros-core-nomysql/Cargo.toml`.
5. Run `cargo build --workspace` and `cargo audit` from the `rust/` directory.
   Expected: the build succeeds and `cargo audit` reports zero vulnerabilities.

If MySQL or SQLite support is ever needed, remove both forks and the
`[patch.crates-io]` entry in `rust/Cargo.toml`, then upgrade `rsa` via normal
transitive means or accept the advisory after re-evaluating exposure.

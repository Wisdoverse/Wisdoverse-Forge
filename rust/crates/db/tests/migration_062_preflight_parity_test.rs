//! Guards that migration 062's pre-flight invariant assertion stays byte-identical
//! to migration 063's `agents_runtime_kind_invariants` CHECK.
//!
//! Why this exists: 062 runs a `bad_rows` pre-flight that is supposed to surface
//! any agent row that would later violate 063's CHECK, BEFORE 063 hard-locks the
//! constraint. If 062's predicate is weaker than 063's, an offending row passes
//! 062 clean and then detonates at 063's `VALIDATE CONSTRAINT`, leaving a
//! half-applied migration — exactly the failure 062's pre-flight is meant to
//! prevent. (A migration dry-run on a seeded `api`-shaped row with a stale
//! `container_id` caught precisely this drift; see
//! `docs/runbooks/migration-062-runtime-kind.md`.)
//!
//! Run with: `cargo test -p agentforge-db --test migration_062_preflight_parity_test`

use std::fs;
use std::path::PathBuf;

fn migrations_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("migrations")
}

/// Extract the three `runtime_kind = '...'` invariant arms from a migration's
/// SQL, normalized (whitespace collapsed) so formatting differences don't cause
/// false negatives.
fn invariant_arms(sql: &str) -> Vec<String> {
    sql.lines()
        .map(str::trim)
        .filter(|line| line.contains("runtime_kind =") && line.contains("cli_tool"))
        .map(|line| {
            // Strip a leading `OR ` and a trailing `)` wrapper, collapse internal
            // whitespace runs to a single space for a formatting-insensitive compare.
            let stripped = line.trim_start_matches("OR ").trim();
            stripped.split_whitespace().collect::<Vec<_>>().join(" ")
        })
        .collect()
}

#[test]
fn migration_062_preflight_matches_063_invariant() {
    let dir = migrations_dir();
    let preflight = fs::read_to_string(dir.join("062_agents_runtime_kind.sql")).expect("read 062");
    let check = fs::read_to_string(dir.join("063_agents_runtime_kind_check.sql")).expect("read 063");

    let pre_arms = invariant_arms(&preflight);
    let chk_arms = invariant_arms(&check);

    assert!(!pre_arms.is_empty(), "no invariant arms found in 062 — did the pre-flight assertion move/change shape?");
    assert_eq!(
        pre_arms, chk_arms,
        "062 pre-flight invariant arms must be byte-identical to 063's CHECK arms.\n\
         062: {pre_arms:#?}\n063: {chk_arms:#?}\n\
         A weaker 062 predicate lets an offender pass 062 then fail 063's VALIDATE."
    );

    // Belt-and-suspenders: the `api` arm specifically must constrain container_id,
    // since that was the exact historical divergence.
    assert!(
        pre_arms.iter().any(|a| a.contains("runtime_kind = 'api'")
            && a.contains("cli_tool IS NULL")
            && a.contains("container_id IS NULL")),
        "062 api arm must require both cli_tool IS NULL and container_id IS NULL: {pre_arms:#?}"
    );
}

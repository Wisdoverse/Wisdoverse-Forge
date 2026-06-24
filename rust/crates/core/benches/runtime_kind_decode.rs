//! Criterion benchmarks for `RuntimeKind::parse_legacy` and `CliToolKind::parse_legacy`.
//!
//! Motivation: PR #447 introduced a hand-rolled `sqlx::Decode` for `RuntimeKind`
//! that calls `parse_legacy` (string match on a trimmed, lowercased `&str`) once
//! per row.  On the agent-list endpoint this runs once per agent per request.
//! Issue #460 asks for measured cost so we can decide whether optimization is
//! warranted before declaring it "negligible".
//!
//! Benchmark groups
//! ────────────────
//! 1. `parse_single`   — one `parse_legacy` call for each canonical value + a miss.
//! 2. `parse_n_rows`   — simulate a 1 000-row and 10 000-row agent-list decode loop.
//! 3. `as_str`         — round-trip (encode path): `RuntimeKind::as_str()`.
//! 4. `baseline_int`   — trivial integer comparison to contextualize overhead.

use criterion::{BenchmarkId, Criterion, Throughput, black_box, criterion_group, criterion_main};

use agentforge_core::runtime_capability::{CliToolKind, RuntimeKind};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Simulate decoding one `RuntimeKind` from a DB text column value.
#[inline(always)]
fn decode_runtime_kind(raw: &str) -> RuntimeKind {
    RuntimeKind::parse_legacy(raw).unwrap_or(RuntimeKind::Container)
}

/// Simulate decoding one `CliToolKind` from a DB text column value.
#[inline(always)]
fn decode_cli_tool_kind(raw: &str) -> CliToolKind {
    CliToolKind::parse_legacy(raw).unwrap_or(CliToolKind::Claude)
}

// Representative agent-list row payloads — canonical lowercase as stored in DB.
const RUNTIME_INPUTS: &[&str] = &["container", "cli", "api"];
const CLI_TOOL_INPUTS: &[&str] = &["claude", "codex", "gemini", "opencode"];

// A value that will exercise the error/miss path.
const MISS_INPUT: &str = "unknown_runtime_xyz";

// ---------------------------------------------------------------------------
// Benchmark: single parse per canonical value + miss
// ---------------------------------------------------------------------------

fn bench_parse_single(c: &mut Criterion) {
    let mut g = c.benchmark_group("parse_single");

    for &input in RUNTIME_INPUTS {
        g.bench_with_input(BenchmarkId::new("RuntimeKind", input), input, |b, s| {
            b.iter(|| RuntimeKind::parse_legacy(black_box(s)));
        });
    }

    g.bench_function("RuntimeKind/miss", |b| {
        b.iter(|| RuntimeKind::parse_legacy(black_box(MISS_INPUT)).ok());
    });

    for &input in CLI_TOOL_INPUTS {
        g.bench_with_input(BenchmarkId::new("CliToolKind", input), input, |b, s| {
            b.iter(|| CliToolKind::parse_legacy(black_box(s)));
        });
    }

    g.finish();
}

// ---------------------------------------------------------------------------
// Benchmark: N-row agent-list decode loop
// ---------------------------------------------------------------------------

fn bench_parse_n_rows(c: &mut Criterion) {
    let mut g = c.benchmark_group("parse_n_rows");

    // Build a representative row-set: cycle through the 3 runtime values.
    // The agent-list query returns one runtime_kind text per agent row.
    for &n in &[1_000_usize, 10_000_usize] {
        let rows: Vec<&str> = (0..n).map(|i| RUNTIME_INPUTS[i % RUNTIME_INPUTS.len()]).collect();

        g.throughput(Throughput::Elements(n as u64));
        g.bench_with_input(BenchmarkId::new("RuntimeKind", n), &rows, |b, rows| {
            b.iter(|| {
                let mut total: u32 = 0;
                for &raw in rows {
                    // Use the as_str len as a proxy value to prevent the
                    // decode from being optimised away entirely.
                    total = total.wrapping_add(decode_runtime_kind(black_box(raw)).as_str().len() as u32);
                }
                black_box(total)
            });
        });

        // Also bench the CliToolKind path used for the `cli_tool` column.
        let cli_rows: Vec<&str> = (0..n).map(|i| CLI_TOOL_INPUTS[i % CLI_TOOL_INPUTS.len()]).collect();
        g.bench_with_input(BenchmarkId::new("CliToolKind", n), &cli_rows, |b, rows| {
            b.iter(|| {
                let mut total: u32 = 0;
                for &raw in rows {
                    total = total.wrapping_add(decode_cli_tool_kind(black_box(raw)).as_str().len() as u32);
                }
                black_box(total)
            });
        });
    }

    g.finish();
}

// ---------------------------------------------------------------------------
// Benchmark: as_str round-trip (encode path, not the hot decode path)
// ---------------------------------------------------------------------------

fn bench_as_str(c: &mut Criterion) {
    let mut g = c.benchmark_group("as_str");

    for kind in [RuntimeKind::Container, RuntimeKind::Cli, RuntimeKind::Api] {
        g.bench_with_input(BenchmarkId::new("RuntimeKind", kind.as_str()), &kind, |b, k| {
            b.iter(|| black_box(k.as_str()));
        });
    }

    g.finish();
}

// ---------------------------------------------------------------------------
// Benchmark: trivial integer compare — establishes measurement noise floor
// ---------------------------------------------------------------------------

fn bench_baseline_int(c: &mut Criterion) {
    let mut g = c.benchmark_group("baseline");

    g.bench_function("integer_compare", |b| {
        let x: u32 = black_box(42);
        b.iter(|| black_box(x == 42));
    });

    // Simulate N integer comparisons to match the row-loop workload.
    for &n in &[1_000_usize, 10_000_usize] {
        let values: Vec<u32> = (0..n as u32).collect();
        g.throughput(Throughput::Elements(n as u64));
        g.bench_with_input(BenchmarkId::new("int_loop", n), &values, |b, vals| {
            b.iter(|| {
                let mut total: u64 = 0;
                for &v in vals {
                    total = total.wrapping_add(black_box(v) as u64);
                }
                black_box(total)
            });
        });
    }

    g.finish();
}

// ---------------------------------------------------------------------------
// Entry points
// ---------------------------------------------------------------------------

criterion_group!(benches, bench_parse_single, bench_parse_n_rows, bench_as_str, bench_baseline_int,);
criterion_main!(benches);

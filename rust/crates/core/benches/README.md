# agentforge-core benchmarks

## runtime_kind_decode (issue #460)

Measures the cost of `RuntimeKind::parse_legacy` and `CliToolKind::parse_legacy`
on the agent-list hot path (one call per row, per request).

### How to run

```bash
cd rust
cargo bench -p agentforge-core --bench runtime_kind_decode
```

HTML report is written to `rust/target/criterion/`.

### Measured results

Runner: Linux x86-64, optimised (`--release`) profile.

| Benchmark                                   | Median         |
| ------------------------------------------- | -------------- |
| `RuntimeKind::parse_legacy` — canonical hit | ~36–38 ns/call |
| `RuntimeKind::parse_legacy` — miss path     | ~60 ns/call    |
| `CliToolKind::parse_legacy` — canonical hit | ~37–40 ns/call |
| 1 000-row decode loop (RuntimeKind)         | ~33 µs total   |
| 10 000-row decode loop (RuntimeKind)        | ~325 µs total  |
| `as_str` encode path                        | ~1.3 ns        |
| Baseline: single integer compare            | ~0.7 ns        |

### Decision

**No optimization applied.** The decode adds ~33 ns/row. Decoding a 1 000-row
agent list costs ~33 µs end-to-end — less than 0.1 % of a typical PostgreSQL
query latency (0.5–10 ms). The `to_ascii_lowercase()` alloc is real but
negligibly small (≤30 bytes, short-lived) and invisible against DB round-trip
time. Applying a zero-alloc byte-matching fast path would reduce parse cost by
≤20 ns/call while adding code complexity and a parity-test burden; the
cost/benefit is negative.

See `docs/superpowers/specs/host-cli-enrollment-deferred-tracking.md` §460 for
the full decision record.

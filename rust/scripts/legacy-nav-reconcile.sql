-- Reconcile diff: one row per (table_name, drift_rows) tuple.
-- Read-only; safe to run any time.
--
-- The body lives in the `public.legacy_nav_reconcile()` function created in
-- migration 024 — wrapped there so per-table reads can be `to_regclass`-gated
-- (a plain SELECT would fail to parse on a fresh DB without `legacy.*`).
SELECT table_name, drift_rows FROM public.legacy_nav_reconcile();

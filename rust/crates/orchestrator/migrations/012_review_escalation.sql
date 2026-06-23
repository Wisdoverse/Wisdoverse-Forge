-- Review escalation: records when an overdue review was escalated by the reaper (#871, follow-up to #801).
-- `escalated_at` is nullable: NULL = not yet escalated, a timestamp = escalated once (the
-- idempotency guard). The reaper never touches `code_reviews.state` — verdict transitions
-- stay 100% human/MCP-driven. No backfill: existing rows remain NULL.

ALTER TABLE code_reviews ADD COLUMN IF NOT EXISTS escalated_at TIMESTAMPTZ;

-- Partial index for the escalation reaper's sweep: it scans non-terminal, unescalated rows
-- ordered by due_at. Without this index the query full-scans code_reviews on every tick.
CREATE INDEX IF NOT EXISTS idx_code_reviews_overdue_unescalated
  ON code_reviews (due_at)
  WHERE escalated_at IS NULL AND state IN ('pending', 'in_review');

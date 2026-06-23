-- Review SLA: adds a nullable `due_at` column to code_reviews (#801).
-- Populated at create time from ORCHESTRATOR_REVIEW_SLA_SECS (default 24h).
-- No backfill: existing rows remain NULL (visibility-only; no reaper).

ALTER TABLE code_reviews ADD COLUMN IF NOT EXISTS due_at TIMESTAMPTZ;

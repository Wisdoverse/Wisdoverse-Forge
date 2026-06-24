-- 067: per-user UI preferences (defaultCliTool, gettingStartedDismissed, ...)
-- stored as one JSONB document on the users row. The API shallow-merges
-- validated PATCH bodies into this column; '{}'::jsonb means "no preferences
-- set yet". Idempotent so it tolerates pre-existing production drift.

ALTER TABLE users ADD COLUMN IF NOT EXISTS preferences JSONB NOT NULL DEFAULT '{}'::jsonb;

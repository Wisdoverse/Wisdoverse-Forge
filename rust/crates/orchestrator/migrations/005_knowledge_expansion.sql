-- no-transaction
-- Expand knowledge_entries for search, embeddings, and additional types.

BEGIN;

-- Add new columns
ALTER TABLE knowledge_entries
    ADD COLUMN IF NOT EXISTS source_type TEXT NOT NULL DEFAULT '',
    ADD COLUMN IF NOT EXISTS source_ref  TEXT NOT NULL DEFAULT '',
    ADD COLUMN IF NOT EXISTS updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    ADD COLUMN IF NOT EXISTS embedding_status TEXT NOT NULL DEFAULT 'pending'
        CHECK (embedding_status IN ('pending', 'processing', 'completed', 'failed'));

-- Expand the type check constraint to include new entry types.
ALTER TABLE knowledge_entries DROP CONSTRAINT IF EXISTS knowledge_entries_type_check;
ALTER TABLE knowledge_entries ADD CONSTRAINT knowledge_entries_type_check
    CHECK (type IN ('session', 'document', 'snippet', 'session_summary', 'review_learnings', 'decision_record'));

-- Full-text search index
CREATE INDEX IF NOT EXISTS idx_knowledge_fts
    ON knowledge_entries USING gin(to_tsvector('english', title || ' ' || content));

-- Trigger for updated_at
CREATE TRIGGER knowledge_entries_updated_at
    BEFORE UPDATE ON knowledge_entries
    FOR EACH ROW EXECUTE FUNCTION update_updated_at();

COMMIT;

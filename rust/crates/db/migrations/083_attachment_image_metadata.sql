-- Image metadata for attachments used as instruction image input.
--
-- `kind` distinguishes image uploads from generic files so the instruction
-- paths only accept images. `workspace_id` carries the workspace the upload was
-- made in, asserted against the executing agent's workspace at use time
-- (CLAUDE.md execution boundary). `width`/`height` are the decoded, re-encoded
-- dimensions used for the multimodal token estimate and UI preview.
-- `checksum_sha256` supports integrity and dedupe. All nullable / defaulted so
-- existing rows and the generic upload path are unaffected.
ALTER TABLE attachments ADD COLUMN IF NOT EXISTS kind TEXT NOT NULL DEFAULT 'file';
ALTER TABLE attachments ADD COLUMN IF NOT EXISTS workspace_id UUID;
ALTER TABLE attachments ADD COLUMN IF NOT EXISTS width INTEGER;
ALTER TABLE attachments ADD COLUMN IF NOT EXISTS height INTEGER;
ALTER TABLE attachments ADD COLUMN IF NOT EXISTS checksum_sha256 TEXT;

-- Defense-in-depth: only the two known kinds are valid. Idempotent add.
DO $$ BEGIN
    ALTER TABLE attachments ADD CONSTRAINT attachments_kind_check CHECK (kind IN ('file', 'image'));
EXCEPTION
    WHEN duplicate_object THEN NULL;
END $$;

-- Workspace-scoped image lookups for the instruction resolve path.
CREATE INDEX IF NOT EXISTS attachments_workspace_kind_idx ON attachments (workspace_id, kind);

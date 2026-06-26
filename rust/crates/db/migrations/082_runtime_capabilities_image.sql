-- Add supports_image_input to the runtime capability registry and reseed.
--
-- The new column mirrors RuntimeCapability.supports_image_input (vision input).
-- It is added with a conservative default so the ALTER is safe on existing rows;
-- the DELETE then clears every row so RuntimeCapabilityRegistryService::
-- refresh_from_code() re-seeds the table from RuntimeCapability::all() on the
-- next startup, writing the correct per-runtime value into both the scalar
-- column and the capability_profile JSONB. Without the reseed the previously
-- seeded rows would diverge from the code matrix and abort startup.
-- Idempotent: ADD COLUMN IF NOT EXISTS, and DELETE is a no-op when empty.
ALTER TABLE runtime_capabilities
    ADD COLUMN IF NOT EXISTS supports_image_input BOOLEAN NOT NULL DEFAULT false;

DELETE FROM runtime_capabilities;

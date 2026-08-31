-- Down-migration for 020_consumer_unique_name.up.sql
--
-- The up-migration swapped a non-unique index for a unique one. Reverting
-- restores the original index from 013_collection_scanner.up.sql, so the
-- schema returns to exactly its pre-020 shape rather than losing the index
-- entirely.
--
-- NOTE: this widens what the database will accept. Rows that violate the
-- unique constraint cannot exist while 020 is applied, so reverting is safe —
-- but re-applying 020 afterwards will fail if duplicate (org_id, name) pairs
-- were introduced in the meantime.

DROP INDEX IF EXISTS idx_consumer_unique_name_org;

CREATE INDEX IF NOT EXISTS idx_consumer_name_org ON consumer (org_id, name);

-- Down-migration for 008_release_notes.up.sql
--
-- WARNING: dropping these tables DESTROYS their data. Reverting past
-- this migration is a data-loss operation, not merely a schema change.
--

DROP INDEX IF EXISTS idx_release_note_created_at;
DROP INDEX IF EXISTS idx_release_note_diff_id;
DROP TABLE IF EXISTS release_note;

-- Down-migration for 018_release_note_status.up.sql
--

ALTER TABLE release_note DROP COLUMN status;

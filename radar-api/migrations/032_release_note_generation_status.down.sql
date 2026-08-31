-- Down-migration for 032_release_note_generation_status.up.sql
--

ALTER TABLE release_note DROP COLUMN generation_error;
ALTER TABLE release_note DROP COLUMN generation_status;

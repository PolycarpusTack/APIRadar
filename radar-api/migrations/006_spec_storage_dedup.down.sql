-- Down-migration for 006_spec_storage_dedup.up.sql
--

DROP INDEX IF EXISTS idx_diff_transition;
ALTER TABLE spec_version DROP COLUMN spec_yaml;

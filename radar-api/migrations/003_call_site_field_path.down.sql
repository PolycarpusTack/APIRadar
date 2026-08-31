-- Down-migration for 003_call_site_field_path.up.sql
--

DROP INDEX IF EXISTS idx_call_site_field_path;
ALTER TABLE call_site DROP COLUMN field_path;

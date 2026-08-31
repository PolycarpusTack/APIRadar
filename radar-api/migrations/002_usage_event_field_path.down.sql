-- Down-migration for 002_usage_event_field_path.up.sql
--

ALTER TABLE usage_event DROP COLUMN field_path;

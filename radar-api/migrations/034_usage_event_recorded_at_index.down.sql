-- Down-migration for 034_usage_event_recorded_at_index.up.sql
--

DROP INDEX IF EXISTS idx_usage_event_recorded_at;

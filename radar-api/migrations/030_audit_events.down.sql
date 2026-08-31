-- Down-migration for 030_audit_events.up.sql
--
-- WARNING: dropping these tables DESTROYS their data. Reverting past
-- this migration is a data-loss operation, not merely a schema change.
--

DROP INDEX IF EXISTS idx_audit_event_org_time;
DROP TABLE IF EXISTS audit_event;

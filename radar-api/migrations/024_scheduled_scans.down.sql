-- Down-migration for 024_scheduled_scans.up.sql
--
-- WARNING: dropping these tables DESTROYS their data. Reverting past
-- this migration is a data-loss operation, not merely a schema change.
--

DROP TABLE IF EXISTS scheduled_scan;

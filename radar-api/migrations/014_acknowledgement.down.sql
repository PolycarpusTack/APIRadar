-- Down-migration for 014_acknowledgement.up.sql
--
-- WARNING: dropping these tables DESTROYS their data. Reverting past
-- this migration is a data-loss operation, not merely a schema change.
--

DROP INDEX IF EXISTS idx_ack_org_consumer;
DROP INDEX IF EXISTS idx_ack_org_service;
DROP INDEX IF EXISTS idx_ack_org_diff;
DROP TABLE IF EXISTS acknowledgement;

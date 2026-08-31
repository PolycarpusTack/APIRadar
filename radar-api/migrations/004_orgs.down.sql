-- Down-migration for 004_orgs.up.sql
--
-- WARNING: dropping these tables DESTROYS their data. Reverting past
-- this migration is a data-loss operation, not merely a schema change.
--

DROP INDEX IF EXISTS idx_consumer_org_id;
DROP INDEX IF EXISTS idx_service_org_id;
DROP INDEX IF EXISTS idx_org_name;
ALTER TABLE consumer DROP COLUMN org_id;
ALTER TABLE service DROP COLUMN org_id;
DROP TABLE IF EXISTS org;

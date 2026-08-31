-- Down-migration for 015_catalog_source.up.sql
--
-- WARNING: dropping these tables DESTROYS their data. Reverting past
-- this migration is a data-loss operation, not merely a schema change.
--

DROP INDEX IF EXISTS idx_catalog_source_org;
DROP TABLE IF EXISTS catalog_source;

-- Down-migration for 013_collection_scanner.up.sql
--

DROP INDEX IF EXISTS idx_consumer_name_org;
ALTER TABLE consumer DROP COLUMN catalog_source;

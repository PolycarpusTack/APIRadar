-- Down-migration for 001_init.up.sql
--
-- WARNING: dropping these tables DESTROYS their data. Reverting past
-- this migration is a data-loss operation, not merely a schema change.
--

DROP INDEX IF EXISTS idx_call_site_service_id;
DROP INDEX IF EXISTS idx_call_site_consumer_id;
DROP INDEX IF EXISTS idx_usage_event_service_id;
DROP INDEX IF EXISTS idx_usage_event_consumer_id;
DROP INDEX IF EXISTS idx_subscription_consumer_id;
DROP INDEX IF EXISTS idx_subscription_service_id;
DROP INDEX IF EXISTS idx_change_diff_id;
DROP INDEX IF EXISTS idx_diff_to_version;
DROP INDEX IF EXISTS idx_diff_from_version;
DROP INDEX IF EXISTS idx_spec_version_service_id;
DROP TABLE IF EXISTS call_site;
DROP TABLE IF EXISTS usage_event;
DROP TABLE IF EXISTS subscription;
DROP TABLE IF EXISTS consumer;
DROP TABLE IF EXISTS change;
DROP TABLE IF EXISTS diff;
DROP TABLE IF EXISTS spec_version;
DROP TABLE IF EXISTS service;

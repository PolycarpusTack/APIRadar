-- Down-migration for 022_webhook_deliveries.up.sql
--
-- WARNING: dropping these tables DESTROYS their data. Reverting past
-- this migration is a data-loss operation, not merely a schema change.
--

DROP TABLE IF EXISTS webhook_delivery;

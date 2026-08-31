-- Down-migration for 017_service_sampling.up.sql
--
-- WARNING: dropping these tables DESTROYS their data. Reverting past
-- this migration is a data-loss operation, not merely a schema change.
--

DROP TABLE IF EXISTS service_sampling;

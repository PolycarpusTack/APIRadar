-- Down-migration for 005_test_suites.up.sql
--
-- WARNING: dropping these tables DESTROYS their data. Reverting past
-- this migration is a data-loss operation, not merely a schema change.
--

DROP INDEX IF EXISTS idx_test_suite_created;
DROP INDEX IF EXISTS idx_test_suite_service;
DROP TABLE IF EXISTS generated_test_suite;

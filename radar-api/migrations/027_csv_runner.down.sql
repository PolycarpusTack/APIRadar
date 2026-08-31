-- Down-migration for 027_csv_runner.up.sql
--
-- WARNING: dropping these tables DESTROYS their data. Reverting past
-- this migration is a data-loss operation, not merely a schema change.
--

DROP INDEX IF EXISTS idx_csv_run_result_job;
DROP INDEX IF EXISTS idx_csv_run_job_org;
DROP TABLE IF EXISTS csv_run_result;
DROP TABLE IF EXISTS csv_run_job;

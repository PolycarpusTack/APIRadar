-- Down-migration for 028_csv_run_result_body.up.sql
--

ALTER TABLE csv_run_result DROP COLUMN response_body;

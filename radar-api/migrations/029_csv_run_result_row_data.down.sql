-- Down-migration for 029_csv_run_result_row_data.up.sql
--

ALTER TABLE csv_run_result DROP COLUMN row_data;

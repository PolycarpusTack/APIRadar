-- Down-migration for 031_scan_run_status.up.sql
--

ALTER TABLE scheduled_scan DROP COLUMN last_run_error;
ALTER TABLE scheduled_scan DROP COLUMN last_run_status;

-- Add last_run_status and last_run_error to scheduled_scan so users can
-- see whether the most recent execution succeeded or why it failed.
ALTER TABLE scheduled_scan ADD COLUMN last_run_status TEXT;
ALTER TABLE scheduled_scan ADD COLUMN last_run_error  TEXT;

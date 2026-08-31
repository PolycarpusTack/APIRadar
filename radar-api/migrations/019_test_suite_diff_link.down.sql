-- Down-migration for 019_test_suite_diff_link.up.sql
--

DROP INDEX IF EXISTS idx_gts_diff_id;
ALTER TABLE generated_test_suite DROP COLUMN consumer_id;
ALTER TABLE generated_test_suite DROP COLUMN diff_id;

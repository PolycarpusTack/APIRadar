-- H-1: Link generated test suites back to the diff they were generated from
ALTER TABLE generated_test_suite ADD COLUMN diff_id TEXT;
ALTER TABLE generated_test_suite ADD COLUMN consumer_id TEXT;
CREATE INDEX IF NOT EXISTS idx_gts_diff_id ON generated_test_suite (diff_id);

-- Down-migration for 011_impact_evidence.up.sql
--
-- WARNING: dropping these tables DESTROYS their data. Reverting past
-- this migration is a data-loss operation, not merely a schema change.
--

DROP INDEX IF EXISTS idx_impact_evidence_expires_at;
DROP INDEX IF EXISTS idx_impact_evidence_consumer_id;
DROP INDEX IF EXISTS idx_impact_evidence_diff_id;
DROP TABLE IF EXISTS impact_evidence;

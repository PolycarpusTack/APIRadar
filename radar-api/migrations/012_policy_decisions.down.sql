-- Down-migration for 012_policy_decisions.up.sql
--
-- WARNING: dropping these tables DESTROYS their data. Reverting past
-- this migration is a data-loss operation, not merely a schema change.
--

DROP INDEX IF EXISTS idx_policy_decision_created_at;
DROP INDEX IF EXISTS idx_policy_decision_service_id;
DROP INDEX IF EXISTS idx_policy_decision_diff_id;
DROP TABLE IF EXISTS policy_decision;

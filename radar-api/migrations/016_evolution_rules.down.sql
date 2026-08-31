-- Down-migration for 016_evolution_rules.up.sql
--
-- WARNING: dropping these tables DESTROYS their data. Reverting past
-- this migration is a data-loss operation, not merely a schema change.
--

DROP INDEX IF EXISTS idx_evo_rule_org;
DROP TABLE IF EXISTS evolution_rule;

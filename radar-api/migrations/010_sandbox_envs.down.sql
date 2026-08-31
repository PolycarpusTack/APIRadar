-- Down-migration for 010_sandbox_envs.up.sql
--
-- WARNING: dropping these tables DESTROYS their data. Reverting past
-- this migration is a data-loss operation, not merely a schema change.
--

DROP INDEX IF EXISTS idx_sandbox_env_name;
DROP TABLE IF EXISTS sandbox_env;

-- Down-migration for 026_sandbox_env_org.up.sql
--

DROP INDEX IF EXISTS idx_sandbox_env_org;
ALTER TABLE sandbox_env DROP COLUMN org_id;

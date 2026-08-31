-- Add org_id to sandbox_env so environments are tenant-scoped.
-- Existing rows default to '' (accessible only to unauthenticated / same-org callers).
ALTER TABLE sandbox_env ADD COLUMN org_id TEXT NOT NULL DEFAULT '';

CREATE INDEX IF NOT EXISTS idx_sandbox_env_org ON sandbox_env (org_id);

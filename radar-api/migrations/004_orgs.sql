-- D-4: Multi-org data model — every service and consumer belongs to an org.
-- The org_id is populated from the JWT `org_id` claim on write; empty string
-- indicates pre-migration legacy rows (accessible to all authenticated callers).
-- Compatible with SQLite and PostgreSQL.

CREATE TABLE IF NOT EXISTS org (
    id         TEXT NOT NULL PRIMARY KEY,
    name       TEXT NOT NULL,
    created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_org_name ON org (name);

-- Add org_id to service (non-nullable with empty-string default for existing rows).
ALTER TABLE service ADD COLUMN org_id TEXT NOT NULL DEFAULT '';

-- Add org_id to consumer (same sentinel default).
ALTER TABLE consumer ADD COLUMN org_id TEXT NOT NULL DEFAULT '';

CREATE INDEX IF NOT EXISTS idx_service_org_id  ON service  (org_id);
CREATE INDEX IF NOT EXISTS idx_consumer_org_id ON consumer (org_id);

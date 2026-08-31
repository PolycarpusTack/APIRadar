-- Promote the non-unique index on (org_id, name) to a UNIQUE constraint.
-- This enables atomic ON CONFLICT upserts and prevents duplicate consumer
-- registrations under concurrent scanner runs.
-- Compatible with SQLite and PostgreSQL.

DROP INDEX IF EXISTS idx_consumer_name_org;

CREATE UNIQUE INDEX IF NOT EXISTS idx_consumer_unique_name_org ON consumer (org_id, name);

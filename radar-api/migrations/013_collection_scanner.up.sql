-- E-7: Collection File Scanner support.
-- catalog_source tracks how a consumer was registered: manual | collection_file | codeowners | backstage
-- Compatible with SQLite and PostgreSQL.

ALTER TABLE consumer ADD COLUMN catalog_source TEXT NOT NULL DEFAULT 'manual';

CREATE INDEX IF NOT EXISTS idx_consumer_name_org ON consumer (org_id, name);

-- 006: Spec YAML storage and diff deduplication.
-- spec_yaml stores the raw OpenAPI/AsyncAPI spec text alongside each spec version
-- so tests and release notes can be re-generated without re-uploading the spec.
-- The unique index on diff(from_version, to_version) prevents duplicate diff records
-- for the same schema transition (idempotent check submissions).
-- Compatible with SQLite and PostgreSQL.

ALTER TABLE spec_version ADD COLUMN spec_yaml TEXT;

CREATE UNIQUE INDEX IF NOT EXISTS idx_diff_transition
    ON diff (from_version, to_version);

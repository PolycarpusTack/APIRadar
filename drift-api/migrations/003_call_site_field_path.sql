-- Add field_path to call_site so the scanner can record which specific
-- response/request fields it found in consumer source code.
-- SQLite and PostgreSQL compatible.
ALTER TABLE call_site ADD COLUMN field_path TEXT NOT NULL DEFAULT '';

CREATE INDEX IF NOT EXISTS idx_call_site_field_path ON call_site (field_path);

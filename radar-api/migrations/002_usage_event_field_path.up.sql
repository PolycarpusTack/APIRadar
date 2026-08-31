-- Add field_path column to usage_event for tracking specific fields accessed.
-- SQLite and PostgreSQL compatible.
ALTER TABLE usage_event ADD COLUMN field_path TEXT NOT NULL DEFAULT '';

-- F-4: catalog_source table — tracks configured data sources for consumer ownership metadata.
-- Each row represents one import configuration (Backstage instance, CODEOWNERS file, etc.).
CREATE TABLE IF NOT EXISTS catalog_source (
    id TEXT PRIMARY KEY,
    org_id TEXT NOT NULL DEFAULT '',
    kind TEXT NOT NULL,           -- 'backstage' | 'codeowners' | 'csv' | 'manual'
    name TEXT NOT NULL,           -- human label, e.g. "Internal Backstage"
    url TEXT NOT NULL DEFAULT '', -- for backstage: base URL; for codeowners: repo URL
    token_env TEXT DEFAULT NULL,  -- environment variable name holding the auth token (NOT the token itself)
    sync_interval_secs INTEGER NOT NULL DEFAULT 3600,
    last_sync_at TEXT DEFAULT NULL,
    last_sync_status TEXT DEFAULT NULL, -- 'ok' | 'error' | NULL
    last_sync_error TEXT DEFAULT NULL,
    -- created_at is bound by the application (RFC3339 UTC). No strftime() default:
    -- that function is SQLite-only and aborts CREATE TABLE on PostgreSQL.
    created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_catalog_source_org ON catalog_source (org_id);

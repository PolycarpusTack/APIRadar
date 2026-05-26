CREATE TABLE IF NOT EXISTS scheduled_scan (
  id               TEXT PRIMARY KEY,
  org_id           TEXT NOT NULL DEFAULT '',
  service_id       TEXT NOT NULL,
  spec_url         TEXT NOT NULL,
  format           TEXT NOT NULL DEFAULT 'openapi',
  interval_minutes INTEGER NOT NULL DEFAULT 60,
  last_run_at      TEXT,
  last_spec_hash   TEXT,
  active           INTEGER NOT NULL DEFAULT 1,
  created_at       TEXT NOT NULL
);

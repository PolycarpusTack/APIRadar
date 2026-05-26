CREATE TABLE IF NOT EXISTS audit_event (
  id          TEXT PRIMARY KEY,
  org_id      TEXT NOT NULL DEFAULT 'default',
  actor       TEXT NOT NULL,
  action      TEXT NOT NULL,
  entity_type TEXT,
  entity_id   TEXT,
  meta        TEXT,
  created_at  TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_audit_event_org_time
  ON audit_event(org_id, created_at DESC);

CREATE TABLE IF NOT EXISTS webhook_delivery (
  id           TEXT PRIMARY KEY,
  webhook_id   TEXT NOT NULL REFERENCES webhook(id) ON DELETE CASCADE,
  event        TEXT NOT NULL,
  payload      TEXT NOT NULL,
  status       TEXT NOT NULL DEFAULT 'pending',
  attempt      INTEGER NOT NULL DEFAULT 0,
  error        TEXT,
  delivered_at TEXT
);

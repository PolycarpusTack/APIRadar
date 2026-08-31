CREATE TABLE IF NOT EXISTS webhook (
  id          TEXT PRIMARY KEY,
  org_id      TEXT NOT NULL DEFAULT '',
  url         TEXT NOT NULL,
  events      TEXT NOT NULL DEFAULT 'diff.created',
  secret      TEXT NOT NULL,
  active      INTEGER NOT NULL DEFAULT 1,
  created_at  TEXT NOT NULL
);

CREATE TABLE csv_run_job (
  id             TEXT PRIMARY KEY,
  org_id         TEXT NOT NULL DEFAULT '',
  name           TEXT NOT NULL DEFAULT '',
  request_json   TEXT NOT NULL,
  status         TEXT NOT NULL DEFAULT 'pending',
  total_rows     INTEGER NOT NULL,
  completed_rows INTEGER NOT NULL DEFAULT 0,
  error_count    INTEGER NOT NULL DEFAULT 0,
  error_message  TEXT,
  created_at     TEXT NOT NULL,
  started_at     TEXT,
  completed_at   TEXT
);
CREATE INDEX idx_csv_run_job_org ON csv_run_job (org_id, created_at DESC);

CREATE TABLE csv_run_result (
  id            TEXT PRIMARY KEY,
  job_id        TEXT NOT NULL REFERENCES csv_run_job(id) ON DELETE CASCADE,
  row_number    INTEGER NOT NULL,
  http_status   INTEGER,
  duration_ms   INTEGER NOT NULL,
  error         TEXT,
  url           TEXT NOT NULL,
  created_at    TEXT NOT NULL
);
CREATE INDEX idx_csv_run_result_job ON csv_run_result (job_id, row_number ASC);

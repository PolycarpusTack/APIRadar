-- E-1: Durable, append-only evidence records produced during blast-radius computation.
-- Compatible with both SQLite and PostgreSQL.
-- source_type: runtime_usage | static_call_site | contract_test | manual_ack
-- confidence:  high | medium | low
-- expires_at:  NULL means no expiry; set by retention policy or admin override.

CREATE TABLE IF NOT EXISTS impact_evidence (
    id                  TEXT NOT NULL PRIMARY KEY,
    org_id              TEXT NOT NULL DEFAULT '',
    diff_id             TEXT NOT NULL,
    change_id           TEXT NOT NULL DEFAULT '',
    producer_service_id TEXT NOT NULL DEFAULT '',
    consumer_id         TEXT NOT NULL,
    source_type         TEXT NOT NULL,
    operation           TEXT,
    field_path          TEXT,
    confidence          TEXT NOT NULL,
    evidence_uri        TEXT,
    file_path           TEXT,
    line_number         INTEGER,
    observed_at         TEXT NOT NULL,
    expires_at          TEXT,
    metadata_json       TEXT
);

CREATE INDEX IF NOT EXISTS idx_impact_evidence_diff_id     ON impact_evidence (diff_id);
CREATE INDEX IF NOT EXISTS idx_impact_evidence_consumer_id ON impact_evidence (consumer_id);
CREATE INDEX IF NOT EXISTS idx_impact_evidence_expires_at  ON impact_evidence (expires_at);

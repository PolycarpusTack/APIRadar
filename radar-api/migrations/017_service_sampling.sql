-- G-6: Per-service ingestion sampling configuration.
-- sample_rate: 0.0 (drop all) to 1.0 (keep all). Applied during OTLP and gateway log ingest.
-- field_deny_list: comma-separated field path patterns to suppress (e.g. "password,secret,**.token").
CREATE TABLE IF NOT EXISTS service_sampling (
    service_id      TEXT NOT NULL,
    org_id          TEXT NOT NULL DEFAULT '',
    sample_rate     REAL NOT NULL DEFAULT 1.0,
    field_deny_list TEXT NOT NULL DEFAULT '',
    updated_at      TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    PRIMARY KEY (service_id, org_id)
);

-- G-6: Per-service ingestion sampling configuration.
-- sample_rate: 0.0 (drop all) to 1.0 (keep all). Applied during OTLP and gateway log ingest.
-- field_deny_list: comma-separated field path patterns to suppress (e.g. "password,secret,**.token").
CREATE TABLE IF NOT EXISTS service_sampling (
    service_id      TEXT NOT NULL,
    org_id          TEXT NOT NULL DEFAULT '',
    sample_rate     REAL NOT NULL DEFAULT 1.0,
    field_deny_list TEXT NOT NULL DEFAULT '',
    -- updated_at is bound by the application (RFC3339 UTC). No strftime() default:
    -- that function is SQLite-only and aborts CREATE TABLE on PostgreSQL.
    updated_at      TEXT NOT NULL,
    PRIMARY KEY (service_id, org_id)
);

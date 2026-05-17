-- API Contract Drift Monitor — initial schema
-- Compatible with both SQLite and PostgreSQL.
-- All primary keys are UUIDs stored as TEXT.
-- All timestamps are ISO 8601 strings stored as TEXT.

-- -------------------------------------------------------------------------
-- service
-- -------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS service (
    id          TEXT NOT NULL PRIMARY KEY,
    name        TEXT NOT NULL,
    repo_url    TEXT NOT NULL,
    owner_team  TEXT NOT NULL,
    spec_format TEXT NOT NULL
);

-- -------------------------------------------------------------------------
-- spec_version
-- -------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS spec_version (
    id          TEXT NOT NULL PRIMARY KEY,
    service_id  TEXT NOT NULL,
    git_ref     TEXT NOT NULL,
    captured_at TEXT NOT NULL,
    spec_format TEXT NOT NULL,
    FOREIGN KEY (service_id) REFERENCES service (id)
);

CREATE INDEX IF NOT EXISTS idx_spec_version_service_id ON spec_version (service_id);

-- -------------------------------------------------------------------------
-- diff
-- -------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS diff (
    id           TEXT NOT NULL PRIMARY KEY,
    from_version TEXT NOT NULL,
    to_version   TEXT NOT NULL,
    pr_url       TEXT,
    created_at   TEXT NOT NULL,
    FOREIGN KEY (from_version) REFERENCES spec_version (id),
    FOREIGN KEY (to_version)   REFERENCES spec_version (id)
);

CREATE INDEX IF NOT EXISTS idx_diff_from_version ON diff (from_version);
CREATE INDEX IF NOT EXISTS idx_diff_to_version   ON diff (to_version);

-- -------------------------------------------------------------------------
-- change
-- -------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS change (
    id          TEXT NOT NULL PRIMARY KEY,
    diff_id     TEXT NOT NULL,
    path        TEXT NOT NULL,
    kind        TEXT NOT NULL,
    severity    TEXT NOT NULL,
    description TEXT,
    FOREIGN KEY (diff_id) REFERENCES diff (id)
);

CREATE INDEX IF NOT EXISTS idx_change_diff_id ON change (diff_id);

-- -------------------------------------------------------------------------
-- consumer
-- -------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS consumer (
    id         TEXT NOT NULL PRIMARY KEY,
    name       TEXT NOT NULL,
    repo_url   TEXT NOT NULL,
    owner_team TEXT NOT NULL,
    contact    TEXT NOT NULL
);

-- -------------------------------------------------------------------------
-- subscription
-- -------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS subscription (
    id          TEXT NOT NULL PRIMARY KEY,
    service_id  TEXT NOT NULL,
    consumer_id TEXT NOT NULL,
    opted_in_at TEXT NOT NULL,
    FOREIGN KEY (service_id)  REFERENCES service  (id),
    FOREIGN KEY (consumer_id) REFERENCES consumer (id)
);

CREATE INDEX IF NOT EXISTS idx_subscription_service_id  ON subscription (service_id);
CREATE INDEX IF NOT EXISTS idx_subscription_consumer_id ON subscription (consumer_id);

-- -------------------------------------------------------------------------
-- usage_event — raw API call telemetry ingested from consumers
-- -------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS usage_event (
    id          TEXT NOT NULL PRIMARY KEY,
    consumer_id TEXT NOT NULL,
    service_id  TEXT NOT NULL,
    operation   TEXT NOT NULL,
    recorded_at TEXT NOT NULL,
    FOREIGN KEY (consumer_id) REFERENCES consumer (id),
    FOREIGN KEY (service_id)  REFERENCES service  (id)
);

CREATE INDEX IF NOT EXISTS idx_usage_event_consumer_id ON usage_event (consumer_id);
CREATE INDEX IF NOT EXISTS idx_usage_event_service_id  ON usage_event (service_id);

-- -------------------------------------------------------------------------
-- call_site — specific code locations where a consumer calls an operation
-- -------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS call_site (
    id           TEXT NOT NULL PRIMARY KEY,
    consumer_id  TEXT NOT NULL,
    service_id   TEXT NOT NULL,
    operation    TEXT NOT NULL,
    file_path    TEXT NOT NULL,
    line_number  INTEGER,
    last_seen_at TEXT NOT NULL,
    FOREIGN KEY (consumer_id) REFERENCES consumer (id),
    FOREIGN KEY (service_id)  REFERENCES service  (id)
);

CREATE INDEX IF NOT EXISTS idx_call_site_consumer_id ON call_site (consumer_id);
CREATE INDEX IF NOT EXISTS idx_call_site_service_id  ON call_site (service_id);

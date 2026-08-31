-- E: Generated Postman test suites, one row per generation request.
CREATE TABLE IF NOT EXISTS generated_test_suite (
    id              TEXT PRIMARY KEY,
    service_id      TEXT,
    jira_key        TEXT,
    jira_summary    TEXT,
    collection_name TEXT NOT NULL,
    collection_json TEXT NOT NULL,
    test_count      INTEGER NOT NULL DEFAULT 0,
    happy_count     INTEGER NOT NULL DEFAULT 0,
    negative_count  INTEGER NOT NULL DEFAULT 0,
    created_at      TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_test_suite_service ON generated_test_suite (service_id);
CREATE INDEX IF NOT EXISTS idx_test_suite_created ON generated_test_suite (created_at);

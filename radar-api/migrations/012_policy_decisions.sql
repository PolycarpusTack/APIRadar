-- E-3-T2: Policy Decision — persisted verdict from every `drift check` run.
-- verdict:   pass | warn | block | overridden
-- fail_mode: closed | open | warn
-- actor:     who triggered the run (e.g. "radar-cli", "radar-action")
-- Compatible with SQLite and PostgreSQL.

CREATE TABLE IF NOT EXISTS policy_decision (
    id          TEXT NOT NULL PRIMARY KEY,
    org_id      TEXT NOT NULL DEFAULT '',
    diff_id     TEXT,
    service_id  TEXT,
    verdict     TEXT NOT NULL,
    fail_mode   TEXT NOT NULL,
    actor       TEXT,
    created_at  TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_policy_decision_diff_id    ON policy_decision (diff_id);
CREATE INDEX IF NOT EXISTS idx_policy_decision_service_id ON policy_decision (service_id);
CREATE INDEX IF NOT EXISTS idx_policy_decision_created_at ON policy_decision (created_at);

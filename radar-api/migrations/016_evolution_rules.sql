-- Evolution rules allow operators to downgrade specific ChangeKind severities
-- (e.g. treat field_added as safe even when the default severity is non_breaking_risky).
-- Rules are org-scoped, append-only in practice, and evaluated server-side at query time.
CREATE TABLE IF NOT EXISTS evolution_rule (
    id           TEXT PRIMARY KEY,
    org_id       TEXT NOT NULL DEFAULT '',
    name         TEXT NOT NULL,
    change_kind  TEXT NOT NULL,
    path_pattern TEXT DEFAULT NULL,
    severity_override TEXT NOT NULL CHECK (severity_override IN ('safe', 'non_breaking_risky')),
    enabled      INTEGER NOT NULL DEFAULT 1,
    created_at   TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);
CREATE INDEX IF NOT EXISTS idx_evo_rule_org ON evolution_rule (org_id, enabled);

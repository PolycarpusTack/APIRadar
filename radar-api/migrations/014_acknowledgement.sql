-- F-3: acknowledgement table — formal record that a consumer owner, producer,
--       or platform team has reviewed and accepted a specific Breaking Change impact.
CREATE TABLE IF NOT EXISTS acknowledgement (
    id TEXT PRIMARY KEY,
    org_id TEXT NOT NULL DEFAULT '',
    diff_id TEXT,                  -- which diff this acknowledgement covers (NULL = service-wide)
    change_id TEXT,                -- specific change_id within the diff (NULL = all changes)
    consumer_id TEXT,              -- which consumer is acknowledging (NULL = producer-side ack)
    service_id TEXT,               -- producer service affected
    acknowledged_by TEXT NOT NULL, -- actor: user identity or system (e.g. 'ci/label:drift-ack')
    reason TEXT,                   -- free-text rationale recorded for audit
    expires_at TEXT,               -- ISO8601 UTC; NULL = no expiry
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

CREATE INDEX IF NOT EXISTS idx_ack_org_diff ON acknowledgement (org_id, diff_id);
CREATE INDEX IF NOT EXISTS idx_ack_org_service ON acknowledgement (org_id, service_id);
CREATE INDEX IF NOT EXISTS idx_ack_org_consumer ON acknowledgement (org_id, consumer_id);

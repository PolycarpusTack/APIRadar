-- Revert N-29: collapse settings back to a single global key/value store.
--
-- Only the default-scope ('') rows survive: a global table cannot represent
-- per-org values, so any tenant-specific settings written since the up
-- migration are dropped rather than silently applied to everyone.
CREATE TABLE settings_old (
    key   TEXT NOT NULL PRIMARY KEY,
    value TEXT NOT NULL
);

INSERT INTO settings_old (key, value)
SELECT key, value FROM settings WHERE org_id = '';

DROP TABLE settings;

ALTER TABLE settings_old RENAME TO settings;

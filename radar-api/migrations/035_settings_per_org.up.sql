-- N-29: scope application settings to an org.
--
-- The settings table was a single global key/value store, so ANY tenant could
-- rewrite policy.block_on or retention.days for the whole installation — and
-- the retention job then purged every org's history on that one value.
--
-- Existing rows migrate to org_id = '' — the single-tenant / no-auth scope
-- (CallerOrg::SingleTenant's sql_scope), so current installs keep their
-- settings unchanged.
--
-- Table rebuild rather than ALTER: the primary key becomes composite, and
-- rebuild-copy-rename is the one form both SQLite and PostgreSQL accept.
CREATE TABLE settings_new (
    org_id TEXT NOT NULL DEFAULT '',
    key    TEXT NOT NULL,
    value  TEXT NOT NULL,
    PRIMARY KEY (org_id, key)
);

INSERT INTO settings_new (org_id, key, value)
SELECT '', key, value FROM settings;

DROP TABLE settings;

ALTER TABLE settings_new RENAME TO settings;

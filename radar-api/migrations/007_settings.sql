-- Persistent key/value store for user-configurable application settings.
CREATE TABLE IF NOT EXISTS settings (
    key   TEXT NOT NULL PRIMARY KEY,
    value TEXT NOT NULL
);

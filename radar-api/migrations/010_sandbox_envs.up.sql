-- Shared Playground environments (replaces per-browser localStorage)
CREATE TABLE IF NOT EXISTS sandbox_env (
    id           TEXT NOT NULL PRIMARY KEY,
    name         TEXT NOT NULL,
    base_url     TEXT NOT NULL DEFAULT '',
    bearer_token TEXT NOT NULL DEFAULT '',
    description  TEXT NOT NULL DEFAULT '',
    created_at   TEXT NOT NULL,
    updated_at   TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_sandbox_env_name ON sandbox_env (name);

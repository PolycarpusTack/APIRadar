-- Stores AI-generated release notes produced by `radar explain --release-notes`.
CREATE TABLE IF NOT EXISTS release_note (
    id         TEXT NOT NULL PRIMARY KEY,
    diff_id    TEXT NOT NULL,
    content    TEXT NOT NULL,
    created_at TEXT NOT NULL,
    FOREIGN KEY (diff_id) REFERENCES diff (id)
);

CREATE INDEX IF NOT EXISTS idx_release_note_diff_id    ON release_note (diff_id);
CREATE INDEX IF NOT EXISTS idx_release_note_created_at ON release_note (created_at DESC);

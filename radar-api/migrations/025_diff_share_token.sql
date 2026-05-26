ALTER TABLE diff ADD COLUMN share_token TEXT;
CREATE UNIQUE INDEX IF NOT EXISTS idx_diff_share_token ON diff(share_token) WHERE share_token IS NOT NULL;

-- Down-migration for 025_diff_share_token.up.sql
--

DROP INDEX IF EXISTS idx_diff_share_token;
ALTER TABLE diff DROP COLUMN share_token;

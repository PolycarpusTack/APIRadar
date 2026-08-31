-- Down-migration for 023_webhook_type.up.sql
--

ALTER TABLE webhook DROP COLUMN type;

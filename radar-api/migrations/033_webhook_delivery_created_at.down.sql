-- Down-migration for 033_webhook_delivery_created_at.up.sql
--

DROP INDEX IF EXISTS idx_webhook_delivery_webhook_created;
DROP INDEX IF EXISTS idx_webhook_delivery_status;
DROP INDEX IF EXISTS idx_webhook_delivery_webhook;
ALTER TABLE webhook_delivery DROP COLUMN created_at;

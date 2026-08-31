-- 033: webhook_delivery ordering + hot-path indices (M-7).
-- The delivery list previously used `ORDER BY rowid` (SQLite-only; 500s on
-- PostgreSQL). Add a portable `created_at` column to order by, plus indices for
-- the per-webhook lookup and the pending-delivery poll performed by the outbox.
-- Portable across SQLite and PostgreSQL: TEXT column, constant default, no
-- SERIAL/TIMESTAMPTZ/strftime.

ALTER TABLE webhook_delivery ADD COLUMN created_at TEXT NOT NULL DEFAULT '';

CREATE INDEX IF NOT EXISTS idx_webhook_delivery_webhook ON webhook_delivery(webhook_id);
CREATE INDEX IF NOT EXISTS idx_webhook_delivery_status ON webhook_delivery(status);
CREATE INDEX IF NOT EXISTS idx_webhook_delivery_webhook_created ON webhook_delivery(webhook_id, created_at);

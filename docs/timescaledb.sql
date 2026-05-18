-- D-5: TimescaleDB hypertable for usage_event (opt-in, PostgreSQL only).
--
-- Prerequisites:
--   1. PostgreSQL 14+ with TimescaleDB 2.x extension installed.
--   2. Standard sqlx migrations (001–004) already applied.
--   3. Run this script manually — it is NOT part of the automatic migration chain
--      because it requires the TimescaleDB extension and will fail on SQLite.
--
-- Apply:
--   psql $DATABASE_URL -f docs/timescaledb.sql
--
-- This converts the usage_event table into a TimescaleDB hypertable partitioned
-- by recorded_at (1-week chunks). Existing data is preserved.

BEGIN;

-- Enable the extension (requires superuser or rds_superuser).
CREATE EXTENSION IF NOT EXISTS timescaledb CASCADE;

-- Convert usage_event to a hypertable.
-- recorded_at is stored as ISO-8601 TEXT; cast it to TIMESTAMPTZ for partitioning.
-- Step 1: add a timestamptz column derived from the text column.
ALTER TABLE usage_event ADD COLUMN IF NOT EXISTS recorded_at_ts TIMESTAMPTZ;
UPDATE usage_event SET recorded_at_ts = recorded_at::TIMESTAMPTZ WHERE recorded_at_ts IS NULL;
ALTER TABLE usage_event ALTER COLUMN recorded_at_ts SET NOT NULL;

-- Step 2: create hypertable (chunk_time_interval = 7 days).
SELECT create_hypertable(
    'usage_event',
    'recorded_at_ts',
    chunk_time_interval => INTERVAL '7 days',
    if_not_exists => TRUE,
    migrate_data => TRUE
);

-- Step 3: optional compression policy (compress chunks older than 30 days).
ALTER TABLE usage_event SET (
    timescaledb.compress,
    timescaledb.compress_orderby = 'recorded_at_ts DESC',
    timescaledb.compress_segmentby = 'service_id, consumer_id'
);

SELECT add_compression_policy('usage_event', INTERVAL '30 days', if_not_exists => TRUE);

-- Step 4: retention policy — drop chunks older than 365 days.
SELECT add_retention_policy('usage_event', INTERVAL '365 days', if_not_exists => TRUE);

COMMIT;

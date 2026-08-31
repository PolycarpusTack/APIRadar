-- 034: index usage_event.recorded_at (N-37).
-- The retention purge (`DELETE ... WHERE recorded_at < ?`) and blast-radius
-- recency lookbacks scan usage_event — the hottest table — by recorded_at with
-- no supporting index. Portable across SQLite and PostgreSQL (TEXT column).

CREATE INDEX IF NOT EXISTS idx_usage_event_recorded_at ON usage_event (recorded_at);

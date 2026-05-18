# Drift Monitor — Runbook

> **Audience:** On-call engineers and DevOps.  
> **Last updated:** 2026-05-18

---

## Table of Contents

1. [Architecture overview](#1-architecture-overview)
2. [Standard deploy](#2-standard-deploy)
3. [Rollback procedure](#3-rollback-procedure)
4. [Database operations](#4-database-operations)
5. [Alerts and escalation](#5-alerts-and-escalation)
6. [Common incidents](#6-common-incidents)
7. [Secrets and credentials](#7-secrets-and-credentials)

---

## 1. Architecture overview

```
Browser / Electron
       │
       ▼
drift-ui (Vite/React, static)
       │  served by drift-api at /app
       ▼
drift-api (axum, port 8080)
       │
       ├── SQLite (Electron / local dev)  ← sqlite:drift.db
       └── PostgreSQL 16 (production)    ← postgres://…
              │
              └── drift-scanner (background worker, reads source dirs)
```

Key env vars:

| Variable | Purpose | Default |
|---|---|---|
| `DATABASE_URL` | DB connection string | `sqlite:drift.db` |
| `BIND_ADDR` | Listen address | `0.0.0.0:8080` |
| `STATIC_DIR` | Path to drift-ui `dist/` | _(none — UI not served)_ |
| `DRIFT_SERVICE_TOKEN` | Static Bearer token for v1 routes | _(none — auth disabled)_ |
| `DRIFT_JWT_SECRET` | HS256 secret for JWT auth (overrides static token) | _(none)_ |
| `RATE_LIMIT_PER_MINUTE` | Max requests/min per client IP (0 = off) | `300` |
| `ANTHROPIC_API_KEY` | Enables Claude-powered release notes | _(none)_ |

---

## 2. Standard deploy

### Docker Compose (production web)

```bash
# Pull latest images and restart
docker compose pull
docker compose up -d --remove-orphans

# Verify health
curl http://localhost:8080/health
```

### Migrations (run automatically on start-up)

Migrations live in `drift-api/migrations/` and are applied by sqlx on boot.  
To run manually:

```bash
DATABASE_URL=postgres://drift:drift_dev@localhost/drift \
  sqlx migrate run --source drift-api/migrations
```

### Binary (bare-metal)

```bash
./drift-api \
  --db postgres://drift:secret@pg-host/drift \
  --static-dir /srv/ui/dist \
  --bind 0.0.0.0:8080 \
  --rate-limit 500
```

---

## 3. Rollback procedure

### Docker Compose

```bash
# Pin to the previous image tag
docker compose down
sed -i 's/drift-api:latest/drift-api:v0.1.0/' docker-compose.yml
docker compose up -d
```

### Database rollback

There are **no** down-migrations. Roll back by:

1. Restore database from the pre-deploy snapshot (see §4).
2. Deploy the previous binary.

---

## 4. Database operations

### Create a snapshot (PostgreSQL)

```bash
pg_dump -Fc drift > drift-$(date +%Y%m%d%H%M).dump
```

### Restore a snapshot

```bash
pg_restore -d drift drift-20260518.dump
```

### Manual retention purge

```bash
# Delete usage_event rows older than 90 days
psql $DATABASE_URL -c \
  "DELETE FROM usage_event WHERE recorded_at < NOW() - INTERVAL '90 days';"
```

---

## 5. Alerts and escalation

| Alert | Threshold | Action |
|---|---|---|
| `drift-api` unreachable | `/health` returning non-200 for 2 min | Restart container; check DB connectivity |
| Error rate > 5 % | Monitor `5xx` responses | Check `RUST_LOG=error` output; possible DB connection exhaustion |
| DB disk > 80 % | — | Enable TimescaleDB compression (see `docs/timescaledb.sql`) or increase retention |
| High blast radius | > 20 consumers affected | Ping producer team's on-call channel |

---

## 6. Common incidents

### "drift-api won't start"

1. Check DB connectivity: `psql $DATABASE_URL -c 'SELECT 1'`
2. Check migration status: `sqlx migrate info --source drift-api/migrations`
3. Check port conflict: `ss -tlnp | grep 8080`

### "CLI returns 401"

1. Verify `DRIFT_SERVICE_TOKEN` matches the server-side value.
2. If `DRIFT_JWT_SECRET` is set, the static token is ignored — use a valid HS256 JWT instead.
3. Generate a test token (requires `jwt-cli`):
   ```bash
   jwt encode --secret "$DRIFT_JWT_SECRET" '{"sub":"ops","org_id":"default","exp":9999999999}'
   ```

### "Blast radius shows 0 consumers despite active consumers"

1. Verify consumers are subscribed: `GET /v1/services/{id}/consumers`
2. Verify usage events are being ingested: `GET /v1/summary`
3. If using static scanner: re-run `drift scan` after indexing new call sites.

### "Scanner finds no call sites"

1. Check that `SOURCE_DIR` contains `.ts`, `.py`, or `.go` files.
2. Excluded directories: `node_modules`, `vendor`, `target`, `.git`, `dist`, `build`.

---

## 7. Secrets and credentials

| Secret | Storage | Rotation |
|---|---|---|
| `DRIFT_SERVICE_TOKEN` | Environment variable / secret manager | Rotate quarterly |
| `DRIFT_JWT_SECRET` | Environment variable / secret manager | Rotate on suspected compromise |
| `ANTHROPIC_API_KEY` | Environment variable / secret manager | Rotate quarterly |
| DB password | Environment variable / secret manager | Rotate quarterly |
| `GITHUB_TOKEN` (CI) | GitHub Actions secret | Managed by GitHub org admins |

**Never** log any of the above values. The server explicitly omits them from all log lines.

---

_Generated by Drift Monitor team. File issues at the internal issue tracker._

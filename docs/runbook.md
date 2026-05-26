# Radar Monitor — Runbook

> **Audience:** On-call engineers and DevOps.  
> **Last updated:** 2026-05-26

---

## Table of Contents

1. [Architecture overview](#1-architecture-overview)
2. [Standard deploy](#2-standard-deploy)
3. [Rollback procedure](#3-rollback-procedure)
4. [Database operations](#4-database-operations)
5. [Prometheus metrics](#5-prometheus-metrics)
6. [Security notes](#6-security-notes)
7. [Alerts and escalation](#7-alerts-and-escalation)
8. [Common incidents](#8-common-incidents)
9. [Secrets and credentials](#9-secrets-and-credentials)

---

## 1. Architecture overview

```
Browser (web mode)              Electron (desktop mode)
       │                                │
       ▼                                ▼
radar-ui (Vite/React, static)   radar-ui (renderer, same bundle)
       │  served by radar-api at /app   │  loaded from Vite dev server or asar
       ▼                                ▼
radar-api (axum, port 8080)     radar-api sidecar (port 17380, 127.0.0.1 only)
       │                                │
       ├── PostgreSQL 16 (production)   └── SQLite ← AppData/radar-desktop/drift.db
       └── SQLite (local dev)
              │
              └── radar-scanner (background worker, reads source dirs)
```

**Port summary:**

| Mode | Port | Bind |
|---|---|---|
| Web / Docker | `8080` | `0.0.0.0` (nginx in front) |
| Desktop sidecar | `17380` | `127.0.0.1` only |
| UI dev server (standalone) | `6173` | localhost only |
| UI dev server (Electron) | `5173` | localhost only |

Key env vars:

| Variable | Purpose | Default |
|---|---|---|
| `DATABASE_URL` | DB connection string | `sqlite:drift.db` |
| `BIND_ADDR` | Listen address | `0.0.0.0:8080` |
| `STATIC_DIR` | Path to radar-ui `dist/` | _(none — UI not served)_ |
| `RADAR_SERVICE_TOKEN` | Static Bearer token for v1 routes | _(none — auth disabled)_ |
| `RADAR_JWT_SECRET` | HS256 secret for JWT auth (overrides static token) | _(none)_ |
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

Migrations live in `radar-api/migrations/` and are applied by sqlx on boot.  
To run manually:

```bash
DATABASE_URL=postgres://drift:radar_dev@localhost/drift \
  sqlx migrate run --source radar-api/migrations
```

### Binary (bare-metal)

```bash
./radar-api \
  --db postgres://drift:secret@pg-host/drift \
  --static-dir /srv/ui/dist \
  --bind 0.0.0.0:8080 \
  --rate-limit 500
```

### Desktop app (Electron installer)

The desktop app bundles `radar-api.exe` as a sidecar. The installer is built from `radar-desktop/`.

**Prerequisites:** Rust release binary must exist before packaging.

```bash
# 1. Build radar-api release binary
cargo build -p radar-api --release
# Binary lands at: target/release/radar-api[.exe]

# 2. Build Electron bundles
cd radar-desktop
pnpm run build

# 3. Package installer
pnpm run dist
# Output: radar-desktop/dist/radar-desktop Setup <version>.exe  (Windows)
#         radar-desktop/dist/radar-desktop-<version>.dmg        (macOS)
#         radar-desktop/dist/radar-desktop-<version>.AppImage   (Linux)
```

**Sidecar resolution order (dev mode):**

1. `RADAR_API_BIN` env var (explicit override)
2. `<resources>/radar-api[.exe]` (packaged installer)
3. `target/release/radar-api[.exe]` (workspace release build — dev shortcut)
4. `cargo run --bin radar-api` (last resort — slow, requires Rust on PATH)

**Known port conflict:** the sidecar binds to `127.0.0.1:17380`. If that port is taken, set `RADAR_API_BIN` to a pre-started binary or kill the conflicting process. Do not use port `8080` — it is commonly reserved by Hyper-V/WSL2/Docker on Windows.

---

## 3. Rollback procedure

### Docker Compose

```bash
# Pin to the previous image tag
docker compose down
sed -i 's/radar-api:latest/radar-api:v0.1.0/' docker-compose.yml
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

## 5. Prometheus metrics

Metrics are available at `GET /metrics` (Prometheus text format, no auth required).

Key metrics:

| Metric | Meaning |
|---|---|
| `request_duration_seconds` | HTTP request latency histogram (all routes) |
| `radar_rate_limit_rejections_total` | Requests rejected by per-IP rate limiter |
| `radar_diffs_created_total` | Total diff computations |
| `radar_consumers_created_total` | Total consumer registrations |
| `radar_test_suites_created_total` | Total test suite generations |

Example Prometheus `scrape_configs` entry:
```yaml
- job_name: radar-api
  static_configs:
    - targets: ['radar-api-host:8080']
  metrics_path: /metrics
```

---

## 6. Security notes

### Rate limiter and reverse proxy

The per-IP rate limiter reads the client IP from `X-Forwarded-For` (first value) or `X-Real-IP`, falling back to `"unknown"`. Clients that control their own headers can spoof these values to bypass rate limiting.

**Mitigation:** deploy `radar-api` behind a reverse proxy (nginx, Caddy) and configure the proxy to **overwrite** `X-Forwarded-For` with the real peer address rather than appending to a client-supplied value.

Example nginx snippet:
```nginx
location / {
    proxy_set_header X-Forwarded-For $remote_addr;  # overwrite, not append
    proxy_set_header X-Real-IP       $remote_addr;
    proxy_pass http://radar-api:8080;
}
```

Without this, rate limiting is advisory only (it deters casual flooding, not a determined attacker).

### Auth in production

- Set `RADAR_REQUIRE_AUTH=true` in any internet-facing deployment.
- Use `RADAR_JWT_SECRET` (not the static `RADAR_SERVICE_TOKEN`) for multi-tenant deployments; the JWT `org_id` claim scopes all reads and writes.
- `RADAR_JWT_SECRET` and `RADAR_SERVICE_TOKEN` are never logged by the API.

### Sandbox environment tokens

Bearer tokens stored in Sandbox Environments (Settings → Playground) are masked in all API responses — only the last 4 characters are visible. The full token is stored in the database; ensure DB backups are encrypted at rest.

---

## 7. Alerts and escalation

| Alert | Threshold | Action |
|---|---|---|
| `radar-api` unreachable | `/health` returning non-200 for 2 min | Restart container; check DB connectivity |
| Error rate > 5 % | Monitor `5xx` responses | Check `RUST_LOG=error` output; possible DB connection exhaustion |
| DB disk > 80 % | — | Enable TimescaleDB compression (see `docs/timescaledb.sql`) or increase retention |
| High blast radius | > 20 consumers affected | Ping producer team's on-call channel |

---

## 8. Common incidents

### "radar-api won't start"

1. Check DB connectivity: `psql $DATABASE_URL -c 'SELECT 1'`
2. Check migration status: `sqlx migrate info --source radar-api/migrations`
3. Check port conflict: `ss -tlnp | grep 8080`

### "CLI returns 401"

1. Verify `RADAR_SERVICE_TOKEN` matches the server-side value.
2. If `RADAR_JWT_SECRET` is set, the static token is ignored — use a valid HS256 JWT instead.
3. Generate a test token (requires `jwt-cli`):
   ```bash
   jwt encode --secret "$RADAR_JWT_SECRET" '{"sub":"ops","org_id":"default","exp":9999999999}'
   ```

### "Blast radius shows 0 consumers despite active consumers"

1. Verify consumers are subscribed: `GET /v1/services/{id}/consumers`
2. Verify usage events are being ingested: `GET /v1/summary`
3. If using static scanner: re-run `drift scan` after indexing new call sites.

### "Compare Specs returns 422 with parse error"

The `POST /v1/services/:id/diffs/compare` endpoint validates both spec strings before persisting.
The response body includes `{ "spec": "base"|"head", "detail": "..." }` — check which side failed
and verify the pasted content is valid YAML/JSON (OpenAPI), valid SDL (GraphQL), or valid proto3 syntax.
Common causes: missing `openapi:` version field, mismatched indentation, or a BOM character at the
start of a copy-pasted file.

### "Generate release notes returns 404"

`POST /v1/diffs/:id/release-notes/generate` returns 404 if the diff ID does not exist in the database.
This can happen if the diff was stored against a different database instance (e.g. a dev SQLite that
was deleted). Verify the diff ID with `GET /v1/diffs` first.

### "Scanner finds no call sites"

1. Check that `SOURCE_DIR` contains `.ts`, `.py`, or `.go` files.
2. Excluded directories: `node_modules`, `vendor`, `target`, `.git`, `dist`, `build`.

---

## 9. Secrets and credentials

| Secret | Storage | Rotation |
|---|---|---|
| `RADAR_SERVICE_TOKEN` | Environment variable / secret manager | Rotate quarterly |
| `RADAR_JWT_SECRET` | Environment variable / secret manager | Rotate on suspected compromise |
| `ANTHROPIC_API_KEY` | Environment variable / secret manager | Rotate quarterly |
| DB password | Environment variable / secret manager | Rotate quarterly |
| `GITHUB_TOKEN` (CI) | GitHub Actions secret | Managed by GitHub org admins |

**Never** log any of the above values. The server explicitly omits them from all log lines.

---

_Generated by Radar Monitor team. File issues at the internal issue tracker._

# Enterprise Deployment Guide

Self-host Radar Monitor on your own infrastructure with PostgreSQL and OIDC.

## Architecture

```
  Your CI/CD
     |
     | radar-cli  (drift check, scan, generate-tests)
     v
  radar-api  ──── PostgreSQL 16  ──── radar-ui (Nginx/CDN)
  :8080              :5432              :80 / :443
```

`radar-api` is a single stateless binary. All state lives in PostgreSQL. Scale horizontally behind a load balancer — each replica runs the same binary.

## Quick start (Docker Compose)

The bundled `docker-compose.yml` starts `postgres` + `radar-api` on a single host.

```sh
git clone https://github.com/PolycarpusTack/radar-monitor
cd radar-monitor
docker compose up -d
```

| Service | Default URL |
|---|---|
| API + UI | http://localhost:8080/app/ |
| PostgreSQL | localhost:5432 (internal only) |

### Environment variables (minimal)

```sh
# docker-compose.yml already sets DATABASE_URL.
# For production, override with your real credentials:
DATABASE_URL=postgres://radar:STRONG_PASSWORD@db/radar

# Bind to loopback in desktop mode; bind to 0.0.0.0 behind a reverse proxy.
BIND_ADDR=0.0.0.0:8080
```

## Production setup

### 1. PostgreSQL

Radar requires PostgreSQL 14+ with the `pgcrypto` extension (used by `gen_random_uuid()`).

```sql
CREATE DATABASE radar;
CREATE USER radar WITH PASSWORD 'STRONG_PASSWORD';
GRANT ALL PRIVILEGES ON DATABASE radar TO radar;
```

Migrations run automatically on startup. To run them manually:

```sh
sqlx migrate run \
  --source radar-api/migrations \
  --database-url postgres://radar:STRONG_PASSWORD@db/radar
```

### 2. Run the API

```sh
docker run --rm \
  -e DATABASE_URL=postgres://radar:STRONG_PASSWORD@db/radar \
  -e BIND_ADDR=0.0.0.0:8080 \
  -e RADAR_REQUIRE_AUTH=true \
  -p 8080:8080 \
  ghcr.io/polycarpustask/radar-api:latest
```

### 3. OIDC authentication

Radar supports any OIDC provider (Google Workspace, Okta, Azure AD, Keycloak).

```sh
RADAR_REQUIRE_AUTH=true
RADAR_OIDC_PROVIDER_URL=https://accounts.google.com
RADAR_OIDC_CLIENT_ID=<your-client-id>
RADAR_OIDC_CLIENT_SECRET=<your-client-secret>
RADAR_OIDC_REDIRECT_URI=https://radar.example.com/auth/callback

# Claim to use as org_id for multi-tenancy.
# Google Workspace: use "hd" (hosted domain), e.g. "example.com"
# Okta / Azure: use "tid" (tenant ID) or a custom group claim
RADAR_OIDC_ORG_CLAIM=hd
```

See [`docs/oidc-setup.md`](./oidc-setup.md) for provider-specific instructions.

### 4. Static bearer token (simpler alternative to OIDC)

For single-tenant deployments or CI-only access:

```sh
RADAR_REQUIRE_AUTH=true
RADAR_SERVICE_TOKEN=<long-random-secret>
```

All API requests must include `Authorization: Bearer <token>`.

### 5. Reverse proxy (Nginx example)

```nginx
server {
    listen 443 ssl;
    server_name radar.example.com;

    ssl_certificate     /etc/letsencrypt/live/radar.example.com/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/radar.example.com/privkey.pem;

    location / {
        proxy_pass         http://127.0.0.1:8080;
        proxy_set_header   Host              $host;
        proxy_set_header   X-Forwarded-For   $proxy_add_x_forwarded_for;
        proxy_set_header   X-Forwarded-Proto $scheme;
        proxy_read_timeout 60s;
    }
}
```

## Docker Compose (production-ready)

```yaml
services:
  db:
    image: postgres:16-alpine
    restart: unless-stopped
    environment:
      POSTGRES_DB: radar
      POSTGRES_USER: radar
      POSTGRES_PASSWORD: ${RADAR_DB_PASSWORD}
    volumes:
      - pgdata:/var/lib/postgresql/data
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U radar -d radar"]
      interval: 5s
      timeout: 5s
      retries: 10

  api:
    image: ghcr.io/polycarpustask/radar-api:latest
    restart: unless-stopped
    ports:
      - "8080:8080"
    environment:
      DATABASE_URL: postgres://radar:${RADAR_DB_PASSWORD}@db/radar
      BIND_ADDR: 0.0.0.0:8080
      RADAR_REQUIRE_AUTH: "true"
      RADAR_OIDC_PROVIDER_URL: ${RADAR_OIDC_PROVIDER_URL}
      RADAR_OIDC_CLIENT_ID: ${RADAR_OIDC_CLIENT_ID}
      RADAR_OIDC_CLIENT_SECRET: ${RADAR_OIDC_CLIENT_SECRET}
      RADAR_OIDC_REDIRECT_URI: ${RADAR_OIDC_REDIRECT_URI}
      RADAR_OIDC_ORG_CLAIM: hd
      ANTHROPIC_API_KEY: ${ANTHROPIC_API_KEY}
    depends_on:
      db:
        condition: service_healthy

volumes:
  pgdata:
```

Create a `.env` file (never commit it):

```
RADAR_DB_PASSWORD=very-strong-password
RADAR_OIDC_PROVIDER_URL=https://accounts.google.com
RADAR_OIDC_CLIENT_ID=xxx.apps.googleusercontent.com
RADAR_OIDC_CLIENT_SECRET=GOCSPX-xxx
RADAR_OIDC_REDIRECT_URI=https://radar.example.com/auth/callback
ANTHROPIC_API_KEY=sk-ant-xxx
```

## Retention and storage

| Table | Default retention | Configurable |
|---|---|---|
| `usage_event` | 90 days | `RADAR_USAGE_RETENTION_DAYS` |
| `impact_evidence` | 30 days | `lookback_days` in `.radar.yml` |
| `spec_version` | forever | manual `DELETE` |
| `policy_decision` | forever | manual `DELETE` |

The background purge job runs hourly. For high-volume deployments, partition `usage_event` by month using TimescaleDB (`docs/timescaledb.sql`).

## Health check

```sh
curl -s http://radar.example.com/health
# {"status":"ok","db":"ok"}
```

Use this endpoint for load balancer health probes and alerting. See `docs/runbook.md` for alert thresholds and incident runbooks.

## Upgrading

Radar uses append-only, forward-compatible migrations. To upgrade:

```sh
docker compose pull
docker compose up -d
# Migrations run automatically at startup.
```

Rollback: downgrade the image tag; migrations are not reverted automatically. Check the migration changelog in `radar-api/migrations/` for any manual rollback steps.

## Security hardening

- Set `RADAR_REQUIRE_AUTH=true` in all non-local deployments
- Rotate `RADAR_SERVICE_TOKEN` every 90 days if used
- Use TLS termination at the load balancer (never disable TLS for internet-facing deployments)
- `radar-api` binds to `127.0.0.1` when running as Electron sidecar (desktop mode only)
- `ANTHROPIC_API_KEY` and `OPENAI_API_KEY` are never logged or returned in API responses
- Run `cargo audit` on every release build (already wired in CI)
- SBOM (CycloneDX JSON) is generated automatically for every `main` build

See [`docs/security-and-privacy.md`](./security-and-privacy.md) for the full security posture.

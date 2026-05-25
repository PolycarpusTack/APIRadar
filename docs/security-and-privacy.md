# Security and Privacy

This document describes how Radar Monitor handles authentication, authorisation, data
isolation, and sensitive-data controls for the runtime evidence collection pipeline.

---

## Authentication

### OIDC (web / production mode)

Radar Monitor supports OpenID Connect for the web dashboard and API.

Configure the provider via environment variables:

```bash
RADAR_OIDC_ISSUER=https://accounts.example.com
RADAR_OIDC_CLIENT_ID=radar-monitor
RADAR_OIDC_CLIENT_SECRET=...    # never commit this value
```

When OIDC is configured:
- The browser dashboard requires an active session cookie (`/auth/me` returns the JWT claims).
- The `/auth/login` endpoint starts the OIDC Authorization Code flow.
- The `/auth/logout` endpoint clears the session cookie.
- All API endpoints that mutate data (`POST`, `PUT`, `PATCH`, `DELETE`) require a valid
  session or Bearer token.
- Read-only endpoints (`GET`) are accessible unauthenticated when OIDC is not configured,
  enabling headless CLI and CI use without ceremony.

### Bearer tokens (API / CLI / SDK)

For machine-to-machine access, pass a Bearer token in the `Authorization` header:

```
Authorization: Bearer <token>
```

Tokens are validated against the OIDC issuer's JWKS endpoint. The `org_id` claim in the
JWT is used for data isolation (see below).

When OIDC is **not** configured (desktop mode, local dev), the API accepts requests
without authentication.

---

## Authorisation and multi-tenant isolation

Every resource in Radar Monitor is scoped to an `org_id`:

| Resource | Isolation key |
|----------|---------------|
| Service, Consumer, Diff, Change | `org_id` |
| Evolution rules | `org_id` |
| Sampling configuration | `org_id` |
| Evidence records | Filtered by service/consumer owned by `org_id` |
| Policy decisions, Acknowledgements | `org_id` |

The `org_id` is extracted from the JWT `org_id` claim. Requests without a JWT use
`org_id = ""` (single-tenant / desktop mode). Rows with `org_id = ""` are **only**
visible to unauthenticated requests.

**No cross-org data leak is possible**: every SQL query in the API includes an
`org_id = $n` predicate.

---

## Desktop mode security

In Electron desktop mode, `radar-api` is spawned as a child process with:
- `--db sqlite:<user-data-path>/drift.db` — the database is local to the user's machine
- Bind address `127.0.0.1` — the API is never reachable from the network

The Electron renderer (BrowserWindow) is configured with:
- `contextIsolation: true`
- `nodeIntegration: false`

All IPC messages between the renderer and the main process are validated before use.
The main process is treated as an untrusted boundary.

---

## Sensitive field handling

### field_deny_list

Use the per-service `field_deny_list` sampling configuration to prevent sensitive field
names from appearing in evidence records:

```http
PUT /v1/services/{service_id}/sampling
{ "field_deny_list": ["user.password_hash", "payment.card_number", "auth.**"] }
```

Glob syntax: `*` matches a single dot-segment, `**` matches zero or more segments.

Events whose `field_path` matches any deny-list pattern are dropped **before** they are
written to the database. They are not logged.

### SDK-side filtering (recommended)

Apply filtering at the SDK layer so sensitive paths never leave the process:

```python
# Python — only record non-sensitive fields
SENSITIVE = {"password", "card_number", "ssn"}
if field_name not in SENSITIVE:
    batcher.push(operation, f"response.{field_name}")
```

### What is stored

Radar Monitor stores:
- `consumer_id` and `service_id` (opaque identifiers, not PII)
- `operation` — HTTP method + route pattern (e.g. `GET /users/{id}`)
- `field_path` — dot-separated schema path (e.g. `user.email`) **if provided**
- `observed_at` — ISO-8601 timestamp

Radar Monitor does **not** store:
- Request or response bodies
- URL query parameters or headers
- User identifiers or IP addresses
- Authentication credentials

---

## Secrets management

| Secret | How to provide |
|--------|----------------|
| `RADAR_OIDC_CLIENT_SECRET` | Environment variable — never hardcode |
| `ANTHROPIC_API_KEY` | Environment variable — never hardcode |
| Database connection string | `--db` CLI flag or `DATABASE_URL` environment variable |
| Bearer token (SDK) | Environment variable, injected at deploy time |

Tokens and database credentials are **never logged**. The API sanitises error messages to
avoid leaking connection string details.

---

## OTLP receiver security

The OTLP trace receiver (`POST /v1/otlp/v1/traces`) is a privileged endpoint — it can
write evidence for any consumer. Protect it with a Bearer token and restrict network
access at the firewall/ingress level.

The receiver only processes **CLIENT spans** (span kind = 3) and only reads the specific
attributes listed in the [runtime usage ingestion guide](./runtime-usage-ingestion.md).
All other span data is discarded.

---

## Data retention

Radar Monitor does not currently enforce automatic evidence TTL. For GDPR compliance or
storage management, run a periodic cleanup:

```sql
-- Remove runtime evidence older than 90 days
DELETE FROM impact_evidence
WHERE source_type = 'runtime_usage'
  AND observed_at < datetime('now', '-90 days');
```

A configurable retention policy via the API is planned for a future release.

---

## Responsible disclosure

To report a security vulnerability, email **security@example.com** with:
1. A description of the vulnerability
2. Steps to reproduce
3. Potential impact

Please allow 72 hours for an initial response. Do not open a public GitHub issue for
security vulnerabilities.

# OIDC Setup Guide

Radar supports OIDC-based authentication for multi-tenant deployments. Each token is scoped to an `org_id` claim so that organizations cannot access each other's data.

## How it works

1. User visits `/auth/login` → redirected to the OIDC provider (Google, Okta, Auth0, …)
2. Provider authenticates user → redirects back to `/auth/callback?code=…`
3. Radar exchanges the code for tokens, fetches user info, extracts `org_id`
4. Radar issues an HS256 session cookie (`radar_session`)
5. Subsequent API requests validate the session cookie and attach `org_id` to every query

## Environment variables

| Variable | Required | Default | Description |
|---|---|---|---|
| `RADAR_OIDC_PROVIDER_URL` | yes | — | Base URL of the OIDC provider (e.g. `https://accounts.google.com`) |
| `RADAR_OIDC_CLIENT_ID` | yes | — | OAuth2 client ID from the provider |
| `RADAR_OIDC_CLIENT_SECRET` | yes | — | OAuth2 client secret |
| `RADAR_OIDC_REDIRECT_URI` | no | `http://localhost:8080/auth/callback` | OAuth2 callback URL (must match provider config) |
| `RADAR_OIDC_ORG_CLAIM` | no | `hd` | OIDC claim used as `org_id`. Defaults to Google Workspace hosted domain (`hd`). Set to `sub` for single-org deployments. |
| `RADAR_JWT_SECRET` | yes | — | HS256 secret for signing session cookies. Use at least 32 random bytes. |
| `RADAR_REQUIRE_AUTH` | no | `false` | Set to `true` to require authentication on all routes. |

## Provider-specific setup

### Google Workspace

1. Go to [Google Cloud Console](https://console.cloud.google.com/) → APIs & Services → Credentials
2. Create an **OAuth 2.0 Client ID** (Web application)
3. Set Authorized redirect URIs: `https://radar.example.com/auth/callback`
4. Copy client ID and secret

```sh
export RADAR_OIDC_PROVIDER_URL=https://accounts.google.com
export RADAR_OIDC_CLIENT_ID=<client-id>.apps.googleusercontent.com
export RADAR_OIDC_CLIENT_SECRET=<client-secret>
export RADAR_OIDC_REDIRECT_URI=https://radar.example.com/auth/callback
export RADAR_OIDC_ORG_CLAIM=hd          # Google Workspace domain
export RADAR_JWT_SECRET=$(openssl rand -base64 32)
```

With `RADAR_OIDC_ORG_CLAIM=hd`, every user from `example.com` gets `org_id=example.com`. Users from `other.com` get `org_id=other.com` and cannot access `example.com` data.

### Okta

1. Applications → Create App Integration → OIDC → Web Application
2. Sign-in redirect URI: `https://radar.example.com/auth/callback`
3. Copy client ID and secret

```sh
export RADAR_OIDC_PROVIDER_URL=https://<your-org>.okta.com
export RADAR_OIDC_CLIENT_ID=<client-id>
export RADAR_OIDC_CLIENT_SECRET=<client-secret>
export RADAR_OIDC_REDIRECT_URI=https://radar.example.com/auth/callback
export RADAR_OIDC_ORG_CLAIM=sub          # or a custom org claim
```

### Auth0

```sh
export RADAR_OIDC_PROVIDER_URL=https://<your-tenant>.auth0.com
export RADAR_OIDC_CLIENT_ID=<client-id>
export RADAR_OIDC_CLIENT_SECRET=<client-secret>
export RADAR_OIDC_REDIRECT_URI=https://radar.example.com/auth/callback
export RADAR_OIDC_ORG_CLAIM=sub
```

## Static bearer token auth (alternative)

For CI or single-org deployments without OIDC:

```sh
export RADAR_SERVICE_TOKEN=your-static-token
export RADAR_REQUIRE_AUTH=true
```

All requests must include `Authorization: Bearer your-static-token`.

## JWT-based auth (for custom integrations)

Issue HS256 JWTs signed with `RADAR_JWT_SECRET`:

```json
{
  "sub": "ci-pipeline",
  "org_id": "example.com",
  "exp": 1716000000
}
```

Pass as `Authorization: Bearer <jwt>`.

## Multi-org isolation

When auth is enabled, every database query filters by `org_id`. Attempting to access a resource belonging to a different org returns `403 Forbidden`.

Test isolation:
```sh
# Org A token
curl -H "Authorization: Bearer $ORG_A_TOKEN" \
  https://radar.example.com/v1/diffs/<org-b-diff-id>
# → 403 Forbidden
```

## Docker Compose example

```yaml
services:
  radar-api:
    image: radar-api:latest
    environment:
      DATABASE_URL: postgres://radar:radar@postgres/radar
      RADAR_REQUIRE_AUTH: "true"
      RADAR_JWT_SECRET: "${RADAR_JWT_SECRET}"
      RADAR_OIDC_PROVIDER_URL: "https://accounts.google.com"
      RADAR_OIDC_CLIENT_ID: "${GOOGLE_CLIENT_ID}"
      RADAR_OIDC_CLIENT_SECRET: "${GOOGLE_CLIENT_SECRET}"
      RADAR_OIDC_REDIRECT_URI: "https://radar.example.com/auth/callback"
      RADAR_OIDC_ORG_CLAIM: "hd"
```

## Security checklist

- [ ] `RADAR_JWT_SECRET` is at least 32 random bytes (`openssl rand -base64 32`)
- [ ] `RADAR_OIDC_CLIENT_SECRET` is stored in a secrets manager (never in version control)
- [ ] `RADAR_OIDC_REDIRECT_URI` matches exactly the URI registered at the provider
- [ ] `RADAR_REQUIRE_AUTH=true` in production
- [ ] HTTPS is enforced between users and radar-api (TLS termination at reverse proxy)
- [ ] Session cookies are `HttpOnly; SameSite=Lax` (enforced by Radar)

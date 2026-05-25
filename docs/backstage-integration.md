# Backstage Integration

Radar can automatically import services from your Backstage catalog as Consumer records, enriching blast-radius results with real ownership data.

## How it works

Radar polls the Backstage catalog API (`/api/catalog/entities?filter=kind=Component`) on a configurable schedule. For each Component:

- `metadata.name` → Consumer name
- `spec.owner` → Consumer owner team
- `catalog_source` = `backstage`

Existing consumers are updated in-place (owner team refreshed). New components are registered automatically.

## Setup

### 1 — Configure the catalog source

```sh
curl -X POST https://radar.example.com/v1/catalog-sources \
  -H "Authorization: Bearer $RADAR_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "kind": "backstage",
    "name": "Internal Backstage",
    "url": "https://backstage.internal.example.com",
    "token_env": "BACKSTAGE_TOKEN",
    "sync_interval_secs": 3600
  }'
```

**Fields:**
- `kind` — must be `backstage`
- `name` — display label
- `url` — base URL of your Backstage instance
- `token_env` — name of the environment variable holding the Backstage auth token (the token is **not** stored in the database — only the variable name)
- `sync_interval_secs` — polling interval (default: `3600`)

### 2 — Set the Backstage token in the radar-api environment

```sh
export BACKSTAGE_TOKEN=your-backstage-service-account-token
```

For Docker deployments, add it to your `docker-compose.yml` or Kubernetes secret.

### 3 — Trigger the first sync

```sh
curl -X POST https://radar.example.com/v1/catalog-sources/<source-id>/sync \
  -H "Authorization: Bearer $RADAR_TOKEN"
```

Response:
```json
{
  "source_id": "abc-123",
  "synced_at": "2026-05-25T12:00:00Z",
  "status": "ok",
  "consumers_upserted": 42,
  "error": null
}
```

### 4 — Automatic polling

Radar-api does not yet run an internal cron job for catalog sync — trigger it via your CI scheduler or a cron job:

```yaml
# .github/workflows/catalog-sync.yml
name: Sync Backstage catalog
on:
  schedule:
    - cron: '0 * * * *'   # every hour
jobs:
  sync:
    runs-on: ubuntu-latest
    steps:
      - name: Trigger Backstage sync
        run: |
          curl -X POST ${{ vars.RADAR_URL }}/v1/catalog-sources/${{ vars.BACKSTAGE_SOURCE_ID }}/sync \
            -H "Authorization: Bearer ${{ secrets.RADAR_TOKEN }}"
```

## Backstage token scopes

The Backstage service account token needs read access to the catalog API:

```yaml
# app-config.yaml (Backstage)
auth:
  serviceToService:
    - subject: radar-monitor
      allowedRoles:
        - catalog-entity-read
```

## CODEOWNERS fallback

If you don't have Backstage, use the `codeowners` source kind instead:

```sh
curl -X POST https://radar.example.com/v1/catalog-sources \
  -H "Authorization: Bearer $RADAR_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "kind": "codeowners",
    "name": "Monorepo CODEOWNERS",
    "url": "https://raw.githubusercontent.com/org/mono/HEAD/CODEOWNERS"
  }'
```

Radar fetches the raw CODEOWNERS file and creates one Consumer per unique `@owner` handle.

CODEOWNERS format supported:
```
# Comment lines are ignored
* @org/platform-team
/api/ @org/api-team @alice
/services/billing/ @org/billing-team
```

## Viewing catalog sources

```sh
curl https://radar.example.com/v1/catalog-sources \
  -H "Authorization: Bearer $RADAR_TOKEN"
```

Response:
```json
{
  "entries": [
    {
      "id": "abc-123",
      "kind": "backstage",
      "name": "Internal Backstage",
      "url": "https://backstage.internal.example.com",
      "sync_interval_secs": 3600,
      "last_sync_at": "2026-05-25T12:00:00Z",
      "last_sync_status": "ok",
      "last_sync_error": null
    }
  ]
}
```

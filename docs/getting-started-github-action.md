# Getting Started — radar-action GitHub Action

This guide walks you through adding API contract drift checking to a GitHub repository in under 15 minutes.

## Prerequisites

- A GitHub repository that contains an OpenAPI YAML/JSON spec, GraphQL SDL, or protobuf file
- (Optional) A running [Radar API instance](enterprise-deployment.md) for blast radius and policy decisions

## Step 1 — Add the workflow file

Create `.github/workflows/api-drift.yml`:

```yaml
name: API Contract Drift Check

on:
  pull_request:
    paths:
      - 'api/**'            # adjust to wherever your spec lives
      - '**/*.yaml'         # or watch all YAML changes

jobs:
  drift-check:
    runs-on: ubuntu-latest
    permissions:
      contents: read
      pull-requests: write  # needed for --post-comment

    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0    # needed to access the base branch

      - name: Fetch base spec from the target branch
        run: |
          git show origin/${{ github.base_ref }}:api/openapi.yaml > /tmp/base.yaml || \
          cp api/openapi.yaml /tmp/base.yaml   # fallback: same spec = no changes

      - name: Check API drift
        id: radar
        uses: PolycarpusTack/radar-monitor/radar-action@main
        with:
          base-spec: /tmp/base.yaml
          head-spec: api/openapi.yaml
          post-comment: 'true'
        env:
          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}

      - name: Print drift summary
        run: |
          echo "Breaking changes: ${{ steps.radar.outputs.breaking-count }}"
          echo "Policy verdict:   ${{ steps.radar.outputs.policy-verdict }}"
```

## Step 2 — Open a PR with a breaking change

Push a commit that removes a field from your spec. The action will:

1. Detect the breaking change
2. Post a PR comment with the field path, kind, and severity
3. Exit with code 1 (blocking the PR) if `fail-mode: closed` (default)

## Step 3 (optional) — Connect to a Radar API instance

With a Radar API instance, the PR comment gains blast-radius evidence: named consumers, last-seen timestamps, and confidence levels.

Add these repository variables (Settings → Secrets and variables → Variables):
- `RADAR_URL` — base URL of your Radar API (e.g. `https://radar.example.com`)
- `RADAR_SERVICE_ID` — UUID of your service in the registry

And one secret:
- `RADAR_TOKEN` — bearer token for the API

Then update the workflow:

```yaml
      - name: Check API drift
        id: radar
        uses: PolycarpusTack/radar-monitor/radar-action@main
        with:
          base-spec: /tmp/base.yaml
          head-spec: api/openapi.yaml
          service-id: ${{ vars.RADAR_SERVICE_ID }}
          radar-url: ${{ vars.RADAR_URL }}
          radar-token: ${{ secrets.RADAR_TOKEN }}
          post-comment: 'true'
        env:
          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
```

## Step 4 (optional) — Register consumers

Consumers self-register to receive blast-radius evidence:

```sh
radar register \
  --api-url https://radar.example.com \
  --service-id <producer-service-id> \
  --consumer-name billing-svc \
  --repo-url https://github.com/org/billing-svc \
  --owner-team team-billing \
  --contact team-billing@example.com
```

## Available inputs

| Input | Required | Default | Description |
|---|---|---|---|
| `base-spec` | yes | — | Path to the old spec file |
| `head-spec` | yes | — | Path to the new spec file |
| `service-id` | no | `""` | Producer service ID in Radar API |
| `radar-url` | no | `""` | Radar API base URL |
| `radar-token` | no | `""` | Bearer token (use a secret) |
| `fail-mode` | no | `closed` | `closed` \| `open` \| `warn` |
| `post-comment` | no | `false` | Post PR comment |
| `spec-format` | no | auto | `openapi` \| `graphql` \| `protobuf` |

## Action outputs

| Output | Description |
|---|---|
| `diff-id` | Diff ID in Radar API |
| `breaking-count` | Number of breaking changes |
| `affected-consumer-count` | Consumers at risk |
| `policy-verdict` | `pass` \| `warn` \| `block` \| `overridden` |
| `dashboard-url` | Radar dashboard link |

## Fail modes

| Mode | Radar API unreachable | Breaking change found |
|---|---|---|
| `closed` (default) | Exit 1 | Exit 1 |
| `open` | Exit 0 with warning | Exit 1 if active consumers |
| `warn` | Exit 0 | Exit 0 with warning |

## Override a block

Add the `drift-ack` label to a PR to override a block verdict (requires `allow_override_with: label:drift-ack` in `.radar.yml`).

Alternatively, create a server-side acknowledgement:

```sh
curl -X POST https://radar.example.com/v1/acknowledgements \
  -H "Authorization: Bearer $RADAR_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "diff_id": "<diff-id-from-action-output>",
    "acknowledged_by": "alice@example.com",
    "reason": "Consumers have been updated to v2 — safe to merge"
  }'
```

Then re-run the CI check.

## Full `.radar.yml` reference

See [policy-reference.md](policy-reference.md) for a complete policy configuration guide.

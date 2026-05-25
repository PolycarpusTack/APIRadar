# radar-action — API Contract Drift Monitor

GitHub Action that detects breaking API contract changes between spec versions and blocks PRs based on blast radius evidence.

## Usage

```yaml
# .github/workflows/api-drift.yml
name: API Contract Drift Check

on:
  pull_request:
    paths:
      - 'api/**'          # adjust to wherever your spec lives

jobs:
  drift-check:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0  # needed to access the base branch spec

      - name: Fetch base spec
        run: git show origin/${{ github.base_ref }}:api/openapi.yaml > /tmp/base.yaml

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

      - name: Print outputs
        run: |
          echo "Breaking changes: ${{ steps.radar.outputs.breaking-count }}"
          echo "Consumers affected: ${{ steps.radar.outputs.affected-consumer-count }}"
          echo "Policy verdict: ${{ steps.radar.outputs.policy-verdict }}"
          echo "Dashboard: ${{ steps.radar.outputs.dashboard-url }}"
```

## Inputs

| Input | Required | Default | Description |
|---|---|---|---|
| `base-spec` | yes | — | Path to the base (old) spec file — OpenAPI YAML/JSON, GraphQL SDL, or `.proto` |
| `head-spec` | yes | — | Path to the head (new) spec file |
| `service-id` | no | `""` | Producer service ID in Radar API (enables blast radius) |
| `radar-url` | no | `""` | Base URL of your Radar API instance |
| `radar-token` | no | `""` | Bearer token for Radar API auth — use a GitHub secret |
| `fail-mode` | no | `closed` | `closed` \| `open` \| `warn` — behavior when Radar API is unreachable |
| `post-comment` | no | `false` | `true` to post/update a PR comment with the drift summary |
| `spec-format` | no | auto | `openapi` \| `graphql` \| `protobuf` — auto-detected from file extension |

## Outputs

| Output | Description |
|---|---|
| `diff-id` | ID of the diff record in Radar API (empty without `radar-url`) |
| `breaking-count` | Number of breaking changes detected |
| `affected-consumer-count` | Number of consumers at risk (requires Radar API) |
| `policy-verdict` | `pass` \| `warn` \| `block` \| `overridden` |
| `dashboard-url` | Link to the Radar dashboard for this diff (empty without `radar-url`) |

## Fail modes

| `fail-mode` | Radar API unreachable | Breaking change found |
|---|---|---|
| `closed` (default) | Exit 1 — block PR | Exit 1 — block PR |
| `open` | Exit 0 with warning | Exit 1 if active consumers, else 0 |
| `warn` | Exit 0 with warning | Exit 0 with warning |

## Policy override

Add the label `drift-ack` to a PR to override a block verdict (requires `allow_override_with: label:drift-ack` in `.radar.yml`).

## Examples

### Warn-only mode (never block CI)

```yaml
- uses: PolycarpusTack/radar-monitor/radar-action@main
  with:
    base-spec: old.yaml
    head-spec: new.yaml
    fail-mode: warn
```

### With PR comment and full blast radius

```yaml
- uses: PolycarpusTack/radar-monitor/radar-action@main
  with:
    base-spec: old.yaml
    head-spec: new.yaml
    service-id: ${{ vars.RADAR_SERVICE_ID }}
    radar-url: ${{ vars.RADAR_URL }}
    radar-token: ${{ secrets.RADAR_TOKEN }}
    post-comment: 'true'
  env:
    GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
```

### Using outputs in downstream steps

```yaml
- name: Run drift check
  id: radar
  uses: PolycarpusTack/radar-monitor/radar-action@main
  with:
    base-spec: old.yaml
    head-spec: new.yaml

- name: Comment on Slack if blocked
  if: steps.radar.outputs.policy-verdict == 'block'
  uses: 8398a7/action-slack@v3
  with:
    status: failure
    text: "${{ steps.radar.outputs.breaking-count }} breaking API changes affect ${{ steps.radar.outputs.affected-consumer-count }} consumer(s)"
```

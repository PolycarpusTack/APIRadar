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
        uses: PolycarpusTack/radar-monitor/radar-action@v0.3.0
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
| `fail-mode` | no | `closed` | `closed` \| `open` \| `warn` — Fail Mode for the check; only applies when the workspace has no `.radar.yml` (see [Fail modes](#fail-modes)) |
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

| `fail-mode` | Radar API unreachable | Breaking Change found (API ok or not configured) |
|---|---|---|
| `closed` (default) | Exit 1 — block PR | Exit per `block_on` policy (default `any_break` → exit 1) |
| `open` | Exit from the local diff + `block_on` policy; verdict `warn` | Exit per `block_on` policy; verdict `warn` |
| `warn` | Exit 0 — never blocks | Exit 0 — never blocks; verdict `warn` |

### How the `fail-mode` input is applied

`radar check` reads its Fail Mode from a policy file (`fail_mode:` in `.radar.yml`), not from a CLI flag. The action bridges the input with this precedence:

- A `.radar.yml` in the workspace root **always wins** — the `fail-mode` input is ignored, and the action logs a warning when the input was set to `open` or `warn`.
- Without a workspace `.radar.yml`, `fail-mode: open` or `warn` makes the action write a minimal policy file (`fail_mode: <value>`) into the runner temp directory and pass it via `radar check --policy`. Your workspace is never modified.
- `fail-mode: closed` needs no file — it is `radar check`'s built-in default.

To combine a Fail Mode with other policy settings (`block_on`, `allow_override_with`, …), set `fail_mode:` in `.radar.yml` instead of using the input.

## Policy override

Add the label `drift-ack` to a PR to override a block verdict (requires `allow_override_with: label:drift-ack` in `.radar.yml`).

## Examples

### Warn-only mode (never block CI)

Applies when the repository has no `.radar.yml`; otherwise set `fail_mode: warn` in that file instead.

```yaml
- uses: PolycarpusTack/radar-monitor/radar-action@v0.3.0
  with:
    base-spec: old.yaml
    head-spec: new.yaml
    fail-mode: warn
```

### With PR comment and full blast radius

```yaml
- uses: PolycarpusTack/radar-monitor/radar-action@v0.3.0
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
  uses: PolycarpusTack/radar-monitor/radar-action@v0.3.0
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

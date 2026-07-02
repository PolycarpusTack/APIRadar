# Demo Scenario: Catching a Breaking Change Before It Ships

This walkthrough shows the full Radar flow in ~5 minutes using the bundled fixtures.

## Scenario

A platform team maintains `payments-api`. A developer opens a PR that removes the `phone` field from `GET /users/{id}`. Two consumers are known to use this field:

- **billing-svc** — reads `phone` daily via OTel-instrumented gRPC calls (high confidence evidence)
- **mobile-gateway** — accesses `response.phone` in a TypeScript client (medium confidence, S2 scan)

Without Radar, the PR would merge and both consumers would break silently in production.

## Prerequisites

- Docker (for the quickest path) or Rust 1.80+ + Node 20+ for local dev
- `curl` and `jq`

## Step 1 — Start Radar

```sh
docker compose up -d
# UI: http://localhost:8080/app/
# API: http://localhost:8080/v1/
```

Or locally:

```sh
cargo run -p radar-api -- --db sqlite:drift.db &
```

## Step 2 — Seed the demo data

```sh
RADAR_URL=http://localhost:8080 bash fixtures/seed-demo.sh
```

This script:
1. Registers `payments-api` as a producer service
2. Posts v1 and v2 OpenAPI specs and triggers a diff
3. Registers `billing-svc` and `mobile-gateway` consumers
4. Seeds runtime usage evidence (billing-svc) and static call-site evidence (mobile-gateway)

## Step 3 — Inspect the diff

```sh
curl -s http://localhost:8080/v1/services/payments-api/diffs | jq '.diffs[0]'
```

Expected:
```json
{
  "id": "...",
  "breaking_count": 1,
  "from_ref": "v1.0.0",
  "to_ref": "v2.0.0"
}
```

## Step 4 — See the blast radius

```sh
DIFF_ID=$(curl -s http://localhost:8080/v1/services/payments-api/diffs | jq -r '.diffs[0].id')
curl -s "http://localhost:8080/v1/diffs/$DIFF_ID/blast-radius" | jq '.entries[] | {consumer: .consumer.name, confidence: .confidence}'
```

Expected:
```json
{"consumer": "billing-svc",    "confidence": "high"}
{"consumer": "mobile-gateway", "confidence": "medium"}
```

## Step 5 — Run drift check (CLI)

```sh
cargo run -p radar-cli -- check \
  --base fixtures/demo-payments-api/v1.yaml \
  --head fixtures/demo-payments-api/v2.yaml \
  --api-url http://localhost:8080
```

Expected output:
```
BREAKING  FieldRemoved  GET /users/{id} -> response.body.phone  (severity: breaking)

Blast Radius -- 2 consumers affected
  billing-svc     high    (runtime_usage)
  mobile-gateway  medium  (static_call_site)

Policy Verdict: BLOCKED
  fail_mode: closed -- breaking changes affect active consumers
  To override: add the "drift-ack" label to your PR
```

Exit code: `1` (blocked).

## Step 6 — View the migration guide

```sh
curl -s "http://localhost:8080/v1/diffs/$DIFF_ID/migration-guide"
```

Returns Markdown with:
- Per-change-kind migration advice
- Evidence table showing which consumers read which fields
- Call-site table with file paths and line numbers

## Step 7 — Simulate the PR comment

With `GITHUB_TOKEN` set and `--post-github-comment` flag, `drift check` posts a PR comment
like `fixtures/expected-pr-comment.md`.

## Step 8 — Acknowledge and override

To unblock the PR after informing consumers:

1. Add label `drift-ack` to the PR (or use the `.radar.yml` `allow_override_with` label)
2. Rerun `drift check` — verdict changes to `OVERRIDDEN` and exits `0`

## What the UI shows

Open `http://localhost:8080/app/`:

- **Dashboard** — summary chips: 1 breaking change, 2 consumers at risk
- **Diffs** — the v1->v2 diff with the phone field removal
- **Evidence Coverage** — billing-svc: high (runtime), mobile-gateway: medium (static)
- **Release Notes** — draft release note generated for the diff (status: draft -> reviewed -> published)

## Integration test

The demo fixtures are covered by the `demo_scenario` integration test suite:

```sh
cargo test -p radar-cli --test demo_scenario
```

All 6 tests must pass green before any EPIC I release.

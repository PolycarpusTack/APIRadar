# Evidence Confidence Reference

Radar scores each piece of evidence with a confidence level that directly controls whether a PR is blocked.

## Confidence levels

| Level | Meaning | Policy effect |
|---|---|---|
| **high** | Runtime evidence observed recently (≤ 7 days) via OTel or gateway logs | Triggers block in `closed` mode |
| **medium** | Runtime evidence older than 7 days, or static evidence with known operation | Triggers block in `closed` mode |
| **low** | Static evidence with unknown operation (S1 scan) | Triggers warn only; never blocks in `closed` mode |

## Evidence sources

### `runtime_usage` — OTel / gateway telemetry

| Sub-type | Confidence | How it's collected |
|---|---|---|
| Fresh OTel trace (≤ 7 days) | **high** | Consumer services emit field-access spans; Radar OTLP receiver ingests them |
| Stale OTel trace (> 7 days) | **medium** | Same pipeline; freshness check happens at blast-radius query time |
| API gateway log | **high** or **medium** | `POST /v1/gateway/logs` from an NGINX/Envoy adapter; same freshness rule |

Ingest path: consumer → OTel Collector → Radar OTLP endpoint (`POST /v1/otlp/traces`) or gateway adapter (`POST /v1/gateway/logs`).

### `static_call_site` — tree-sitter scanner

| Scanner tier | Confidence | When |
|---|---|---|
| S2 — operation-aware (TypeScript) | **medium** | `radar scan` detects API object + verb-prefix method name; derives `GET /users/{id}` from `usersApi.getUserById()` |
| S1 — field-path only (Python, Go) | **low** | `radar scan` detects field accesses but cannot link them to a specific operation |

The scanner runs as `radar scan --source-dir ./src` in CI and posts results to `POST /v1/call-sites`.

### `collection_file` — Postman v2.1 / NativeREST

Confidence is always **medium**. Radar parses test scripts for `pm.response.json().<field>` patterns and links them to the request's operation. Ingest: `radar scan --collection ./tests.postman_collection.json`.

## How confidence affects the policy engine

```
fail_mode: closed  →  block when ANY high or medium evidence record exists for a changed field
fail_mode: warn    →  never block; always warn regardless of confidence
fail_mode: open    →  block only on high confidence; warn on medium; pass on low
```

The policy engine runs `decide(changes, policy, fail_mode, consumers, ...)` in `radar-cli/src/policy.rs`, where `consumers` is a three-state `ConsumerEvidence` (`Affected` / `NoneAffected` / `Unknown`) rather than a boolean — see the `insufficient coverage` verdict in the policy reference.

## Staleness and expiry

Evidence rows in `impact_evidence` have an `expires_at` column. The background job `expire_old_evidence` runs hourly and marks rows older than `lookback_days` (default 30) as expired. Expired rows are excluded from blast-radius queries and confidence scoring.

Use `GET /v1/evidence/coverage` (Evidence Coverage dashboard tab) to see which consumer × service pairs have fresh versus stale evidence, and which source types are active.

## Raising confidence

To raise a consumer from low → medium or medium → high:

1. **Enable OTel**: instrument your service with the Radar Node.js or Python SDK (or any OTel SDK pointed at the Radar OTLP receiver). Fresh runtime traces → high confidence automatically.
2. **Add Postman tests**: run `radar scan --collection` in CI. Tests with response field assertions → medium confidence.
3. **Annotate the TypeScript client**: if the scanner cannot derive an operation, add `// @radar-operation GET /users/{id}` above the call. S2 scan picks it up → medium confidence.

## Evidence coverage report

```sh
# API
curl -s http://localhost:8080/v1/evidence/coverage | jq .

# Dashboard
open http://localhost:8080/app/evidence-coverage
```

The coverage report shows, per consumer × service, the freshest evidence timestamp and source types present. Cells marked **STALE** (>7 days) or **MISSING** indicate gaps that leave blast radius incomplete.

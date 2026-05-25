# Radar Monitor — API Contract Drift Monitor

> **Status:** `active` · **Cluster:** `Quality` · **Surface:** `CLI + CI + Web + Electron`

Diffs OpenAPI / GraphQL / protobuf across versions, names the consumers each breaking change will affect, and blocks PRs that cross the line.

## Architecture

```
  Producer repo                   Consumer repos
  (payments-api)                  (billing-svc, mobile-gateway)
       |                                 |            |
       | git push / PR                   | OTel SDK   | radar scan
       v                                 v            v
  radar-action (CI)            radar-api (:8080)
       |                        /    |    \
       | spec diff              |    |     |
       v                        |    |     |
  radar-api <-- blast radius --'     |     |
       |         (evidence)          |     |
       v                        PostgreSQL  |
  PR comment                               |
  (verdict + blast radius                  |
   + evidence table)              radar-ui (dashboard)
                                   /evidence-coverage
                                   /release-notes
```

Evidence flows from consumers to Radar via three paths:
1. **OTel traces** → OTLP receiver → `impact_evidence` (high confidence)
2. **`radar scan`** → tree-sitter static analysis → `call_site` (medium/low)
3. **Postman collections** → `radar scan --collection` → `impact_evidence` (medium)

## Why it matters
API producers ship breaking changes by accident because they can't see who's still calling the old shape. Existing tools (`openapi-diff`, `buf breaking`) detect the diff but stop there — nobody tells the reviewer "this removes `user.phone`, which `billing-svc` and `mobile-ios` read daily." Radar closes the loop between schema diff and consumer telemetry so PRs carry a named blast radius — backed by evidence from OTel traces, static code scans, and Postman collection files.

## Quick start

### Docker (web self-host)

```sh
# Start the API (PostgreSQL) + dashboard
docker compose up

# UI available at: http://localhost:8080/app/
# API available at: http://localhost:8080/v1/
```

### Local development

**Prerequisites:** Rust 1.80+, Node 22+, pnpm

```sh
# Start the API (SQLite)
cargo run -p radar-api -- --db sqlite:drift.db

# In another terminal — start the UI dev server (proxies /v1 to :8081)
pnpm dev:ui

# CLI
cargo run -p radar-cli -- --help
```

### Run the full test suite

```sh
cargo test --workspace                        # Rust unit + integration tests
pnpm --filter radar-ui typecheck              # TypeScript check
cargo clippy --all-targets -- -D warnings     # Lint (warnings = errors)
```

## 5-minute demo

Try the payments-api breaking-change scenario from a clean clone:

```sh
# 1. Start Radar
docker compose up -d

# 2. Seed demo data (registers services, consumers, and evidence)
RADAR_URL=http://localhost:8080 bash fixtures/seed-demo.sh

# 3. Run drift check
cargo run -p radar-cli -- check \
  --base fixtures/demo-payments-api/v1.yaml \
  --head fixtures/demo-payments-api/v2.yaml \
  --api-url http://localhost:8080
# => BLOCKED: 1 breaking change, 2 consumers at risk

# 4. View the dashboard
open http://localhost:8080/app/
```

See [`docs/demo-scenario.md`](./docs/demo-scenario.md) for the full step-by-step walkthrough.

## Features

| Feature | Command / route |
|---|---|
| OpenAPI / GraphQL / protobuf diff | `radar check --base v1.0 --head v1.1 --spec api.yaml` |
| Blast radius (named consumers + evidence) | `GET /v1/diffs/{id}/blast-radius` |
| Runtime usage evidence (OTel / custom) | `POST /v1/usage/events` |
| Static call-site evidence (tree-sitter) | `radar scan --source-dir ./src` + `POST /v1/call-sites` |
| Collection file evidence (Postman v2.1) | `radar scan --collection ./tests.postman_collection.json` |
| Policy engine (block / warn / open) | `.radar.yml` `fail_mode` + `block_on` |
| GitHub PR comments with evidence table | `GITHUB_TOKEN` + `--post-github-comment` |
| AI release notes + migration guides | `radar explain --release-notes` |
| Consumer registry | `POST /v1/consumers`, `radar register` |
| AI test generation (Postman + api-testing) | `radar generate-tests`, `POST /v1/generate-tests` |
| API Playground (Scalar) | `GET /app/` → Playground tab |
| Jira integration | `JIRA_BASE_URL` + `JIRA_TOKEN` |
| Postman push | `POSTMAN_API_KEY` + `--postman-workspace` |

## Configuration — `.radar.yml`

Place `.radar.yml` in your repo root. All fields are optional; shown with defaults.

```yaml
version: 1

# Which service spec this repo owns (matches the service ID in radar-api)
service: my-payments-api

# Policy: when to block a PR
policy:
  # never | any_break | active_consumers (default)
  block_on: active_consumers
  # Evidence older than this is ignored for blast-radius decisions
  lookback_days: 30
  # GitHub label that, when present on the PR, overrides a block verdict
  allow_override_with: "label:drift-ack"

# How API errors affect the build
# closed (default): API unreachable → block
# open:            API unreachable → use local diff only, warn
# warn:            never block the build, always warn
fail_mode: closed

# Postman / NativeREST collection files to scan automatically (glob patterns)
collection_paths:
  - "**/*.postman_collection.json"
  - "**/*.nativerest_collection.json"
```

## Evidence types

Radar builds blast-radius evidence from three sources, each with a confidence level:

| Source | `source_type` | Confidence | How it works |
|---|---|---|---|
| OTel / runtime telemetry | `runtime_usage` | **high** (< 7 days) / medium | Consumer services emit field-access events; `POST /v1/usage/events` ingests them |
| Tree-sitter static scan | `static_call_site` | **medium** (S2, operation known) / low | `radar scan --source-dir` scans TypeScript, Python, Go source for API field accesses |
| Postman collection file | `collection_file` | **medium** | `radar scan --collection` parses v2.1 collection JSON and extracts field paths from test scripts |

Confidence affects the policy engine: `closed` mode blocks when at least one **high** or **medium** confidence evidence record exists for a consumer that uses a changed field.

## Environment variables

| Variable | Purpose | Default |
|---|---|---|
| `DATABASE_URL` | DB connection (`sqlite:file.db` or `postgres://…`) | `sqlite:drift.db` |
| `BIND_ADDR` | Listen address | `0.0.0.0:8080` |
| `RADAR_REQUIRE_AUTH` | Reject unauthenticated requests (`true`/`1`) | `false` |
| `RADAR_SERVICE_TOKEN` | Static bearer token for API auth | — |
| `RADAR_JWT_SECRET` | HS256 secret for JWT auth (`org_id` claim for tenancy) | — |
| `RADAR_OIDC_PROVIDER_URL` | OIDC provider base URL (e.g. `https://accounts.google.com`) | — |
| `RADAR_OIDC_CLIENT_ID` | OIDC client ID | — |
| `RADAR_OIDC_CLIENT_SECRET` | OIDC client secret | — |
| `RADAR_OIDC_REDIRECT_URI` | OAuth2 callback URL | `http://localhost:8080/auth/callback` |
| `RADAR_OIDC_ORG_CLAIM` | OIDC claim to use as `org_id` | `hd` (Google Workspace domain) |
| `RADAR_REQUEST_TIMEOUT_SECS` | Per-request timeout | `30` |
| `ANTHROPIC_API_KEY` | Claude AI provider | — |
| `OPENAI_API_KEY` | OpenAI provider (fallback) | — |
| `OPENAI_BASE_URL` | Custom base URL for OpenAI-compatible APIs | — |
| `GITHUB_COPILOT_TOKEN` | GitHub Copilot provider (fallback) | — |
| `GITHUB_TOKEN` | GitHub PR comments | — |
| `JIRA_BASE_URL` | Jira instance URL | — |
| `JIRA_EMAIL` | Jira auth email | — |
| `JIRA_TOKEN` | Jira API token | — |
| `POSTMAN_API_KEY` | Push collections to Postman | — |

## Workspace layout

```
radar-core/       Shared Rust types (ChangeKind, Severity, Consumer, Diff, …)
radar-cli/        CLI binary  (cargo run -p radar-cli)
radar-api/        Axum HTTP service (cargo run -p radar-api)
radar-scanner/    tree-sitter code scanner + Postman collection parser
radar-ui/         Vite 6 + React 19 web dashboard
radar-desktop/    Electron 33 shell (wraps radar-ui)
fixtures/         Demo scenario fixtures (payments-api v1/v2, billing-svc, mobile-gateway)
docs/             OpenAPI spec + runbook
```

## API reference

Full OpenAPI 3.0 spec: [`docs/openapi.yaml`](./docs/openapi.yaml)

Key endpoints:

```
GET  /health                              → DB-probed health check
GET  /v1/summary                          → dashboard KPIs
GET  /v1/services                         → list producer services
POST /v1/services/{id}/diffs              → post spec changes from CI
GET  /v1/diffs/{id}/blast-radius          → named blast radius with evidence
POST /v1/usage/events                     → ingest runtime consumer telemetry
POST /v1/call-sites                       → upsert static scanner call sites
POST /v1/consumers/upsert                 → auto-register consumer by name (idempotent)
POST /v1/evidence/collection              → write collection-file evidence (idempotent)
POST /v1/policy-decisions                 → persist policy verdict from a drift check
GET  /v1/settings                         → app settings (policy, retention)
POST /v1/generate-tests                   → AI test generation
GET  /v1/sandbox-envs                     → Playground environments
```

## Docs

| Document | Description |
|---|---|
| [`docs/demo-scenario.md`](./docs/demo-scenario.md) | Step-by-step demo walkthrough (5 minutes) |
| [`docs/enterprise-deployment.md`](./docs/enterprise-deployment.md) | Self-host guide: Docker Compose, PostgreSQL, OIDC |
| [`docs/evidence-confidence.md`](./docs/evidence-confidence.md) | Evidence confidence levels and how they affect policy |
| [`docs/generated-artifacts.md`](./docs/generated-artifacts.md) | Test suites, migration guides, release notes |
| [`docs/security-and-privacy.md`](./docs/security-and-privacy.md) | Security posture, OIDC, org isolation, field deny-list |
| [`docs/runtime-usage-ingestion.md`](./docs/runtime-usage-ingestion.md) | OTel, gateway logs, Node/Python SDKs |
| [`docs/oidc-setup.md`](./docs/oidc-setup.md) | OIDC provider setup (Google, Okta, Azure) |
| [`docs/policy-reference.md`](./docs/policy-reference.md) | Policy engine reference (fail_mode, block_on, evolution rules) |
| [`docs/getting-started-github-action.md`](./docs/getting-started-github-action.md) | GitHub Action setup |
| [`docs/runbook.md`](./docs/runbook.md) | Operations runbook (alerts, migrations, on-call) |
| [`docs/openapi.yaml`](./docs/openapi.yaml) | Full API specification (OpenAPI 3.0) |
| [`SOLUTION_DESIGN.md`](./SOLUTION_DESIGN.md) | Architecture and design decisions |
| [`fixtures/README.md`](./fixtures/README.md) | Demo fixture set explanation |

## Related projects

- `09-Git-Repo-Health-Checker` — shares the "scan many repos, roll up to a dashboard" pattern
- `24-Dependency-Lifecycle-Dashboard` — complementary: tracks libs; this tracks API contracts
- `05-Code-Review-Agent` — PR-comment surface is shared; both annotate diffs with context a human missed

# Radar Monitor — API Contract Drift Monitor

> **Status:** `active` · **Cluster:** `Quality` · **Surface:** `CLI + CI + Web + Electron`

Diffs OpenAPI / GraphQL / protobuf across versions, names the consumers each breaking change will affect, and blocks PRs that cross the line.

## Why it matters
API producers ship breaking changes by accident because they can't see who's still calling the old shape. Existing tools (`openapi-diff`, `buf breaking`) detect the diff but stop there — nobody tells the reviewer "this removes `user.phone`, which `billing-svc` and `mobile-ios` read daily." Radar closes the loop between schema diff and consumer telemetry so PRs carry a named blast radius.

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
cargo test --workspace                        # Rust tests (94 total)
pnpm --filter radar-ui typecheck              # TypeScript check
cargo clippy --all-targets -- -D warnings     # Lint (warnings = errors)
```

## Features

| Feature | Command / route |
|---|---|
| OpenAPI / GraphQL / protobuf diff | `radar check --base v1.0 --head v1.1 --spec api.yaml` |
| Blast radius (named consumers + evidence) | `GET /v1/diffs/{id}/blast-radius` |
| AI release notes + migration guides | `radar explain --release-notes` |
| Consumer registry | `POST /v1/consumers`, `radar register` |
| Runtime usage ingest | `POST /v1/usage/events` |
| Static call-site scan (tree-sitter) | `radar-scanner` + `POST /v1/call-sites` |
| AI test generation (Postman + api-testing) | `radar generate-tests`, `POST /v1/generate-tests` |
| API Playground (Scalar) | `GET /app/` → Playground tab |
| GitHub PR comments | `GITHUB_TOKEN` + `--post-github-comment` |
| Jira integration | `JIRA_BASE_URL` + `JIRA_TOKEN` |
| Postman push | `POSTMAN_API_KEY` + `--postman-workspace` |

## Environment variables

| Variable | Purpose | Default |
|---|---|---|
| `DATABASE_URL` | DB connection (`sqlite:file.db` or `postgres://…`) | `sqlite:drift.db` |
| `BIND_ADDR` | Listen address | `0.0.0.0:8080` |
| `RADAR_REQUIRE_AUTH` | Reject unauthenticated requests (`true`/`1`) | `false` |
| `RADAR_SERVICE_TOKEN` | Static bearer token for API auth | — |
| `RADAR_JWT_SECRET` | HS256 secret for JWT auth (`org_id` claim for tenancy) | — |
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
radar-core/       Shared Rust types (Change, Consumer, Diff, …)
radar-cli/        CLI binary  (cargo run -p radar-cli)
radar-api/        Axum HTTP service (cargo run -p radar-api)
radar-scanner/    tree-sitter background worker
radar-ui/         Vite 6 + React 19 web dashboard
radar-desktop/    Electron 33 shell (wraps radar-ui)
docs/             OpenAPI spec (docs/openapi.yaml)
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
POST /v1/usage/events                     → ingest consumer telemetry
POST /v1/call-sites                       → upsert scanner call sites
GET  /v1/settings                         → app settings (policy, retention)
POST /v1/generate-tests                   → AI test generation
GET  /v1/sandbox-envs                     → Playground environments
```

## Docs

- [`SOLUTION_DESIGN.md`](./SOLUTION_DESIGN.md) — architecture and design decisions
- [`DEVELOPMENT_PLAN.md`](./DEVELOPMENT_PLAN.md) — backlog and EPIC breakdown
- [`docs/openapi.yaml`](./docs/openapi.yaml) — API specification

## Related projects

- `09-Git-Repo-Health-Checker` — shares the "scan many repos, roll up to a dashboard" pattern
- `24-Dependency-Lifecycle-Dashboard` — complementary: tracks libs; this tracks API contracts
- `05-Code-Review-Agent` — PR-comment surface is shared; both annotate diffs with context a human missed

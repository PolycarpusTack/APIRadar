# Radar Monitor — HELP

Radar Monitor (API Contract Drift Monitor) diffs API specs — OpenAPI, GraphQL SDL,
and protobuf — across versions, detects every **Breaking Change**, and names the
**Consumers** each one will hit. That named list, backed by **Evidence** (runtime
usage events, static call sites, Postman collection files), is the **Blast Radius**.
A policy engine turns diff + Blast Radius into a verdict, and the configured
**Fail Mode** decides how that verdict (and API errors) map to your CI exit code —
so PRs from a **Producer** repo are blocked only when they genuinely break someone.

- **Producer** — the service that owns and publishes the API spec.
- **Consumer** — a service or client that calls the Producer's API.
- **Evidence** — a record that a Consumer uses an operation/field (three sources:
  `runtime_usage`, `static_call_site`, `collection_file`).
- **Blast Radius** — the set of Consumers, with Evidence, affected by a change.
- **Breaking Change** — a change classified `Breaking` (removed field, type change,
  optional→required in a request, …).
- **Fail Mode** — `closed` (API error → block), `open` (API error → local diff, warn),
  or `warn` (never block).

---

## Install / build

Prerequisites: Rust 1.80+, Node 20+, pnpm 9+.

```sh
cargo build                 # build all Rust crates (radar-core/cli/api/scanner)
pnpm install                # JS workspaces (radar-ui, radar-desktop)
pnpm build:ui               # build the dashboard bundle → radar-ui/dist
```

Run Rust commands sequentially, never in parallel (disk fills up fast).

The CLI binary is `radar` (`cargo run -p radar-cli -- <subcommand>`, or
`target/debug/radar` after a build). All subcommands honour `NO_COLOR`, and
`check`/`batch` support `--json` for machine-readable output.

---

## Quickstart (local, SQLite)

### 1. Start the API

```sh
cargo run -p radar-api -- --db sqlite:drift.db --bind 127.0.0.1:17380
```

Bind to loopback: with no auth configured the server refuses to start on a
publicly reachable address. To serve `0.0.0.0:8080` instead, set
`RADAR_SERVICE_TOKEN=<token>` (or `RADAR_ALLOW_UNAUTHENTICATED=true` on purpose).
Port 17380 is where the UI dev server proxies API requests.

### 2. Register a Producer

```sh
curl -s -X POST http://127.0.0.1:17380/v1/services \
  -H 'Content-Type: application/json' \
  -d '{"name": "payments-api", "spec_format": "openapi"}'
```

The response contains the service `id` — use it as `--service-id` below.
(If `RADAR_SERVICE_TOKEN` is set, add `-H 'Authorization: Bearer <token>'`.)

### 3. Check two spec versions for drift

```sh
cargo run -p radar-cli -- check \
  --base fixtures/demo-payments-api/v1.yaml \
  --head fixtures/demo-payments-api/v2.yaml \
  --api-url http://127.0.0.1:17380 \
  --service-id <SERVICE_ID>
```

`--base` and `--head` are spec file paths. Format is auto-detected from the file
extension; override with `--format openapi|graphql|protobuf`. Useful flags:

| Flag | Effect |
|---|---|
| `--policy <file>` | Policy file controlling severity thresholds (defaults to `./.radar.yml` when present) |
| `--post-comment` | Post/update a PR comment when running in CI (needs `GITHUB_TOKEN`) |
| `--json` | Machine-readable output |
| `--summary-file <path>` | Write a JSON summary (used by radar-action) |
| `--token <t>` | Bearer token (or `RADAR_SERVICE_TOKEN` env) |

Without `--api-url` the diff is local-only; with it, the diff is posted, the
Blast Radius is fetched, and the policy decision is recorded
(`POST /v1/policy-decisions`).

### 4. Register and scan a Consumer

```sh
# Register a Consumer subscribed to the Producer
cargo run -p radar-cli -- register \
  --api-url http://127.0.0.1:17380 \
  --service-id <SERVICE_ID> \
  --consumer-name billing-svc \
  --repo-url https://github.com/acme/billing-svc \
  --owner-team billing \
  --contact billing@acme.test

# Scan its source for API usage → static_call_site Evidence
cargo run -p radar-cli -- scan \
  --consumer-id <CONSUMER_ID> \
  --service-id <SERVICE_ID> \
  --source-dir ../billing-svc/src \
  --api-url http://127.0.0.1:17380
```

`scan` reads TypeScript, Python, and Go sources. Add
`--operation-map "field=GET /users"` (repeatable) to attach fields to operations,
and `--collection tests.postman_collection.json` (repeatable) to extract
`collection_file` Evidence — collection scans auto-register the Consumer by name.

### 5. Open the dashboard

```sh
pnpm dev:ui        # dev server → http://localhost:6173 (proxies /v1 to :17380)
```

Or serve the built bundle from the API itself:
`--static-dir radar-ui/dist` → `http://<host>:<port>/app/`.
For a Docker/PostgreSQL setup, `docker compose up` serves everything at
`http://localhost:8080/app/` — and
`RADAR_URL=http://localhost:8080 bash fixtures/seed-demo.sh` seeds a full demo
scenario (see `docs/demo-scenario.md`).

---

## Batch checks

Compare many spec pairs listed in a CSV file:

```sh
cargo run -p radar-cli -- batch --csv pairs.csv \
  --api-url http://127.0.0.1:17380 \
  --policy .radar.yml
```

- CSV columns: `base`, `head` (required); `label`, `format`, `service_id` (optional).
- Every row goes through the same policy engine as `check`: `.radar.yml`
  (`block_on`, `fail_mode`) decides the exit code — the aggregate exit is the
  worst verdict across rows, so `fail_mode: warn` cannot fail CI.
- `--policy` defaults to `./.radar.yml`; with `--api-url`, each row's diff is
  posted (rows with a `service_id`) and its policy decision is recorded.
- Batch never fetches Blast Radius, so Consumer coverage counts as Unknown.
- `--json` and `--no-color` behave as in `check`.

---

## Policy (`.radar.yml`)

```yaml
policy:
  block_on: any_break          # never | any_break | active_consumers
  lookback_days: 30            # ignore Evidence older than this
  allow_override_with: "label:drift-ack"
fail_mode: closed              # closed | open | warn
```

See `docs/policy-reference.md` for the full reference.

---

## Other commands

| Command | Purpose |
|---|---|
| `radar explain --diff-id <id> --api-url <url>` | Explain a diff; `--release-notes`, `--migration-guide`, `--post-github-release`, `--out <file>` |
| `radar generate-tests --spec api.yaml --jira PROJ-123` | Generate Postman tests from a Jira ticket + spec; `--out`, `--postman-workspace` |
| `radar rule add|list|delete|toggle|test` | Org-level severity overrides for specific change kinds |
| `radar completions <shell>` | Shell completion script (bash, zsh, fish, powershell, elvish) |

CI integration: `radar-action/` is a composite GitHub Action wrapping
`radar check` (see `docs/getting-started-github-action.md`). Runtime Evidence
SDKs live in `radar-sdk-node/` and `radar-sdk-python/`
(see `docs/runtime-usage-ingestion.md`).

---

## Desktop app (Electron)

```sh
cargo build -p radar-api --release   # sidecar binary (required first)
pnpm dev:desktop                     # dev mode — spawns the sidecar on 127.0.0.1:17380
```

To build an installer: `cd radar-desktop && pnpm run build && pnpm run dist`
(output in `radar-desktop/dist/`). The desktop app uses SQLite at
`AppData/radar-desktop/drift.db` and generates its own service token.

---

## Where to look next

| Document | Contents |
|---|---|
| `README.md` | Architecture, features, environment variables, workspace layout |
| `docs/runbook.md` | Operations: deploy, rollback (`sqlx migrate revert`), metrics, incidents |
| `docs/openapi.yaml` | Full HTTP API specification (OpenAPI 3.0) |
| `docs/demo-scenario.md` | 5-minute end-to-end demo walkthrough |
| `docs/policy-reference.md` | Fail Mode, `block_on`, evolution rules |
| `docs/enterprise-deployment.md` | Docker Compose, PostgreSQL, OIDC self-hosting |

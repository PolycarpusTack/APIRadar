# API Contract Radar Monitor — Development Plan

> **Framework:** AI-Native Software Delivery · Core Spec v1 · GPM v2.1 · Backlog Builder v5.1
> **Backlog built from:** `SOLUTION_DESIGN.md` v0.2
> **Initial execution mode:** PROTOTYPE (P0) → DELIVERY (P1+)
> **Max active EPICs:** 2 (unlock C/D after EPIC A retrospective passes phase gate)
> **WIP limit:** 1 task per developer at all times (sequential pull)

---

## Phase 0 — Domain Foundation

_Architect produces all Phase 0 artifacts before any code task is pulled. Stakeholder validates value hypotheses and smoke-test outlines. No EPIC A tasks start until this section is signed off._

### Domain Glossary

> Domain language is code language (Principle P3). All identifiers, table names, field names, and PR comments must use these exact terms.

| Term | Definition |
|---|---|
| **Producer** | A service that owns and publishes an API spec (OpenAPI / GraphQL SDL / protobuf) |
| **Consumer** | A service that calls a Producer's API and may be broken by schema changes |
| **Spec Version** | An immutable snapshot of a Producer's spec captured at a specific `git_ref` |
| **Diff** | The full typed set of Changes between two Spec Versions |
| **Change** | A single entry in a Diff — identified by `(field_path, kind, severity)` |
| **Breaking Change** | A Change that removes or incompatibly modifies a field/operation a Consumer may depend on |
| **Non-Breaking Change** | An additive or compatible Change (new field, new optional parameter) |
| **Blast Radius** | The set of Consumers affected by one or more Breaking Changes, each with confidence level and last-seen timestamp |
| **Call Site** | A reference to a Producer field found in Consumer source code via tree-sitter extraction |
| **Usage Event** | A runtime telemetry record of a Consumer accessing a specific `field_path` at a given timestamp |
| **Consumer Registry** | The persistent mapping of Consumers subscribed to a specific Producer |
| **Migration Guide** | Per-Consumer Markdown document: what broke, old usage, suggested new shape |
| **Release Notes** | Versioned consumer-facing document summarising all Changes in a Diff |
| **Policy** | Rules that control whether a Diff blocks CI (`block`) or only warns (`warn`) |
| **Playground** | The embedded Scalar interactive API explorer in the dashboard |
| **Subscription** | A registered Consumer-to-Producer relationship (Consumer opted in) |
| **Lookback Window** | Rolling time period (default 30 days) used to determine if a Consumer is "active" |
| **Sandbox Environment** | A named pre-configured environment entry in the Playground (base URL + auth) used for pre-sales demos |
| **Desktop Mode** | `radar-api` running with SQLite, spawned as a child process by `radar-desktop` (Electron), with no external infrastructure |
| **Web Mode** | `radar-api` running with PostgreSQL, deployed on a server, with `radar-ui` served as a static bundle |
| **Evidence** | A durable, append-only record in `impact_evidence` that links a Diff Change to a specific Consumer with a source type, confidence level, and observed timestamp |
| **Confidence** | A signal strength classification (high / medium / low) assigned to an Evidence record based on the recency and type of the evidence source |
| **Policy Decision** | A persisted record in `policy_decision` capturing the verdict (pass / warn / block / overridden), fail mode, and actor for a given Diff evaluation |
| **Acknowledgement** | A formal record that a Consumer owner, producer, or platform team has reviewed and accepted a specific Breaking Change impact, optionally with an expiry date |
| **Artifact** | A generated output tied to a Diff: release_notes, migration_guide, postman_collection, apitesting_yaml, or schemathesis_config; progresses through status: draft → reviewed → published → superseded |
| **Catalog Source** | A configured data source (Backstage, GitHub, CODEOWNERS, CSV, manual) from which Consumer ownership metadata is imported into the service registry |
| **Fail Mode** | The behavior of `radar-action` when the Radar API is unreachable: `closed` (blocks build), `open` (allows build, warns), `warn` (never fails) |
| **Scanner Stage** | The capability level of the static scanner: S0 (field extraction only) through S4 (framework-specific semantic scanning); S2 (operation + field correlation) is the publishable minimum |
| **Demo Scenario** | The canonical three-repo demonstration set (demo-payments-api, demo-billing-svc, demo-mobile-gateway) used to prove the full "field removed → evidence → block" flow |

### Architecture Memory

```
radar-cli        Rust binary — local/CI diff runs, release-notes generation, PR comment posting
                 fail-mode: closed | open | warn (explicit since Phase 1)
radar-action     TypeScript composite GitHub Action — wraps radar-cli for producer repos
                 inputs: base-spec, head-spec, service-id, radar-url, policy, fail-mode, post-comment
                 outputs: diff-id, breaking-count, affected-consumer-count, policy-verdict, dashboard-url
radar-api        Rust/axum HTTP service — compiled once; targets SQLite or PostgreSQL via --db flag
                 Internal modules: diffs | evidence | impact | policy | artifacts | catalog | authz | audit
                 Also serves radar-ui static bundle at /app in web mode
radar-scanner    Rust/tree-sitter background worker — call-site extraction from Consumer repos
                 Scanner stages: S0 (field) → S1 (HTTP client + field) → S2 (operation + field) → S3 (generated-client) → S4 (semantic)
                 Currently at S2 for TypeScript generated clients (Phase 1 target)
radar-ui         Vite 6 + React 19 + Tailwind + shadcn/ui — shared renderer
                 Pages: Services, Diffs, Consumers, Evidence, Policies, Artifacts, Audit, Settings
                 Runs in browser (web) or inside radar-desktop Electron renderer (desktop)
radar-desktop    Electron 33 shell — wraps radar-ui; spawns radar-api sidecar; manages SQLite file
drift-db         SQLite file (local/Electron default) | PostgreSQL 16 (web/production)
                 Tables: service, spec_version, diff, change, consumer, subscription,
                         usage_event, call_site,
                         impact_evidence (append-only), policy_decision, acknowledgement,
                         artifact, catalog_source
                 TimescaleDB optional on usage_event at scale

Dependencies point inward: cli → api; radar-ui → api (HTTP); radar-desktop → radar-ui + radar-api (IPC+HTTP); scanner → api; radar-action → cli; nothing → cli except radar-action
```

### Architectural Decision Records

| ADR | Decision | Rationale | Alternatives rejected |
|---|---|---|---|
| ADR-001 | Rust for all backend components | Spec parsing is CPU-bound per PR; cross-platform binary ships without a runtime | Go (no tree-sitter bindings as mature), Python (too slow for per-PR parsing) |
| ADR-002 | SQLite (local/Electron default) + PostgreSQL 16 (web/production) via sqlx | sqlx `AnyDatabase` lets `radar-api` compile once and target either engine; SQLite gives zero-setup desktop experience; PostgreSQL unlocks HA and TimescaleDB at scale | Two separate ORMs (more code, drift risk), SQLite-only (no web scale path), PostgreSQL-only (kills offline/desktop use) |
| ADR-003 | Scalar (MIT) for API Playground | Zero per-seat cost; renders directly from parsed OpenAPI spec; self-hosted; skinnable | Postman (paid per-seat), Swagger UI (older UX), Redoc (no interactive testing) |
| ADR-004 | Claude for narrative generation only | Diff classification must be deterministic and auditable; only human-readable prose uses LLM | LLM for classification (non-deterministic, hallucination risk on field paths) |
| ADR-005 | tree-sitter for call-site extraction | Language-agnostic, embeddable in Rust, mature grammars for TS/Python/Go/Rust/Java | Regex (brittle), language-specific AST tools (can't unify in one binary) |
| ADR-006 | OTLP-compatible ingest for Usage Events | Consumers already export OTLP; re-using the same pipeline avoids a second SDK | Custom SDK (adoption friction), log scraping (unstructured) |
| ADR-007 | Vite 6 over Next.js 15 for `radar-ui` | Next.js SSR runs in a Node.js process; Electron's renderer is a Chromium page — SSR is incompatible. Vite produces a static bundle that loads identically in a browser or Electron renderer. In web mode, radar-api serves the Vite build from `/app`. | CRA (deprecated), Next.js with `output: 'export'` (loses API routes, SSR, image optimisation — same result as Vite but with more complexity) |
| ADR-008 | Electron 33 for desktop distribution | Cross-platform installers (.exe / .dmg / .AppImage) with one codebase; main process can spawn radar-api as a child process and manage the SQLite file lifecycle; auto-update via electron-updater | Tauri (Rust-native, lighter, but WebView rendering differs per OS which would require significant CSS testing), PWA (no sidecar process management, no offline DB) |
| ADR-009 | electron-vite as the Electron build tool | Same Vite config for `radar-ui` (web) and `radar-desktop` (renderer); HMR in development; no separate webpack config | Plain webpack (no HMR, verbose config), Electron Forge with webpack plugin (heavier, less Vite-native) |
| ADR-010 | `graphql-parser 0.4` for GraphQL SDL parsing | Pure Rust, no external binary, MIT licence; parses SDL into a typed AST sufficient for object/interface/enum/union/input diff; owned-string generic (`parse_schema::<String>`) avoids lifetime propagation across crate boundaries | `async-graphql` (heavier, server-oriented), rolling a hand-written parser (high maintenance for a well-solved problem) |
| ADR-011 | Hand-rolled proto3 parser in `radar-core` (no `protoc`) | Installing `protoc` in CI and on developer machines adds external tool dependency and complicates cross-compilation; our use-case only needs message/enum/field structure — not the full proto descriptor — so a character-scanning parser (~300 LOC) covers 100 % of the test matrix with zero binary dependency | `prost-build` (requires `protoc`; generates Rust structs — not the schema diff model we need), `protobuf` crate (same `protoc` dependency), `protox` (parses, but re-exports `prost-build` model) |
| ADR-012 | `radar-action` packaged as workspace crate `radar-action/` (TypeScript, composite GitHub Action) | GitHub Actions marketplace expects a `action.yml` at repo root or a published action; TypeScript composite action avoids binary distribution complexity; extract to standalone repo on first public release | Node.js action (slower startup), Docker container action (no ARM runners), Rust binary distributed via release (bootstrap problem for new orgs) |
| ADR-013 | OTel integration via custom processor component (not exporter) | Processor can inspect span attributes and emit usage events without requiring a separate exporter endpoint; fits naturally into existing collector pipelines; no changes needed to instrumented services | Custom exporter (requires new endpoint config in every service), direct SDK (adoption friction) |
| ADR-014 | Evidence records are append-only; blast-radius recomputation writes new records rather than updating | Append-only evidence is trivially auditable; expiry is a soft-delete via `expires_at`; recomputation on blast-radius request seeds evidence for first-time callers | Mutable evidence (audit gaps), compute-only blast radius (no durability, no trend data) |
| ADR-015 | Backstage integration via polling importer (HTTP client reads Backstage catalog API) | No Backstage plugin required on consumer side; importer runs as a scheduled job in `radar-api`; org can point Radar at any Backstage instance without Backstage changes | Backstage plugin (requires consumer-side Backstage upgrade), webhook push (requires Backstage config changes) |

### Value Hypotheses (Stakeholder)

| # | Hypothesis | Measurable signal | Validated by |
|---|---|---|---|
| VH-1 | PR comment naming consumers removes the "I didn't know they used it" excuse | Reviewer reassigns or adds ack label on >50% of breaking PRs | P1 dogfood on internal repo |
| VH-2 | Auto-generated release notes cut the "what changed?" back-channel by >30% | Slack thread volume about API changes (proxy: manual changelog commits) | P3 survey |
| VH-3 | Embedded Playground eliminates Postman from pre-sales demo stack | Pre-sales team stops requesting Postman workspace access | P2 sandbox demo |

### Phase 0 Sign-off Checklist

- [ ] Domain Glossary reviewed and approved by Architect + Stakeholder
- [ ] All ADRs recorded — no open architectural decisions
- [ ] Value hypotheses written — measurable signals agreed
- [ ] Stack declaration committed (Rust toolchain version, Node version, PostgreSQL version pinned)
- [ ] Cross-cutting concerns documented: auth model, secret handling, observability pipeline, CI runner image

---

## EPIC A — Tracer Bullet: OpenAPI Diff CLI

> **Mode:** PROTOTYPE
> **Theme:** Thin end-to-end slice — `drift check` parses two OpenAPI YAML files and posts a PR comment listing Breaking Changes. No Blast Radius yet.
> **Tracer bullet:** YES — first EPIC must deploy a working end-to-end path
> **Business value:** Gives a platform team proof that the tool catches breaking changes before merge. Unblocks P1 consumer work.
> **Risk:** OpenAPI spec variety (vendor extensions, $ref nesting) may require more parser work than estimated.
> **SLO:** `drift check` p95 < 5 s on specs ≤ 1 MB
> **Smoke test:** Run `drift check --base v1.yaml --head v2.yaml` on the billing-api fixture; PR comment appears within 30 s; exit code 1 when breaking change present.
> **Exit criteria:**
> - `drift check` runs in CI on one real repo
> - PR comment lists at least one Breaking Change with field path, kind, and severity
> - Exit code 0 / 1 / 2 semantics documented
> - `radar-desktop` launches on Windows and macOS; radar-api sidecar starts with SQLite; radar-ui loads inside the Electron window
> - All tasks pass DoD (80 % test coverage, lint clean, no secrets, no two-hat violations)

---

### Story A-1 · Project Skeleton & CI

> **Persona:** Platform engineer setting up the repo
> **Value:** So that all subsequent tasks start from a clean, runnable baseline with CI already green
> **Priority:** P0 (blocks everything)
> **Size:** S
> **INVEST:** Independent · Negotiable · Valuable · Estimable · Small · Testable
> **DoR status:** READY

**Acceptance Criteria (Gherkin)**

```gherkin
Given the repo is cloned on a fresh machine
When `cargo build` is run
Then radar-cli compiles without errors

Given the repo is cloned
When `pnpm install && pnpm build` is run in drift-dashboard/
Then the Next.js build succeeds

Given a push to any branch
When CI runs
Then cargo test, clippy, and pnpm lint all pass
```

**Tasks**

| ID | Hat | Goal | Agent tier | Token budget |
|---|---|---|---|---|
| A-1-T1 | PREPARATORY | Init Cargo workspace with `radar-cli`, `radar-api`, `radar-scanner` crates; `drift-dashboard` pnpm workspace | Sonnet | ≤ 2 000 |
| A-1-T2 | PREPARATORY | GitHub Actions CI: cargo test + clippy + pnpm lint + pnpm build | Sonnet | ≤ 1 500 |
| A-1-T3 | PREPARATORY | Docker Compose: postgres:16, radar-api, drift-dashboard for local dev | Sonnet | ≤ 1 500 |
| A-1-T4 | PREPARATORY | sqlx migrate setup: `drift-db` crate, initial empty migration, run-on-startup flag | Sonnet | ≤ 1 000 |

**Hand-off artifact:** `README.md` with `cargo build`, `docker compose up`, and CI badge.

---

### Story A-2 · Spike — OpenAPI Parser Selection

> **Type:** SPIKE (throwaway — findings recorded in ADR, code discarded)
> **Persona:** Architect evaluating options
> **Value:** So that Story A-3 starts with a confirmed library choice, not a guess
> **Priority:** P0 (blocks A-3)
> **Size:** S
> **Time-boxed:** 1 day

**Spike questions**
1. Does `oas3` (Rust) handle `$ref` resolution and vendor extensions without panicking on real-world specs?
2. Does `openapiv3` crate support OpenAPI 3.1 discriminators?
3. What is parse time on a 500 KB spec with 300 paths?

**Deliverable:** ADR-007 added to this document with chosen library and version pinned in `Cargo.toml`.

**Agent tier:** Opus (judgment-heavy evaluation)

---

### Story A-3 · Parse OpenAPI YAML → Typed Diff

> **Persona:** Platform engineer running `drift check`
> **Value:** So that the tool produces a structured list of Changes I can inspect before any CI wiring
> **Priority:** P0
> **Size:** M
> **Dependencies:** A-2 (parser chosen)
> **DoR status:** READY after A-2 spike complete

**Acceptance Criteria**

```gherkin
Given base.yaml defines field `user.phone` and head.yaml removes it
When `drift diff --base base.yaml --head head.yaml --json` is run
Then the output contains a Change with path="user.phone", kind="field_removed", severity="breaking"

Given head.yaml adds a new optional field `user.nickname`
When the diff runs
Then the output contains a Change with kind="field_added", severity="non_breaking"

Given a spec with circular $refs
When the diff runs
Then it exits with code 2 (parse error) and a human-readable message, not a panic
```

**Tasks**

| ID | Hat | Goal | Agent tier | Token budget |
|---|---|---|---|---|
| A-3-T1 | FEATURE | `SpecVersion` struct + `parse_openapi(path) -> Result<SpecVersion>` with $ref resolution | Sonnet | ≤ 3 000 |
| A-3-T2 | FEATURE | `diff(base: &SpecVersion, head: &SpecVersion) -> Vec<Change>` — field removal, addition, type change, required→optional, optional→required | Sonnet | ≤ 3 000 |
| A-3-T3 | FEATURE | `classify_severity(change: &Change) -> Severity` — breaking vs non-breaking rules per OpenAPI semantics | Sonnet | ≤ 2 000 |
| A-3-T4 | FEATURE | JSON and human-readable table output renderers | Sonnet | ≤ 1 500 |

**Contract snapshot (public interface after this story):**
```rust
pub struct Change { pub path: String, pub kind: ChangeKind, pub severity: Severity }
pub fn diff(base: &SpecVersion, head: &SpecVersion) -> Vec<Change>
```

**TDD order:** write failing tests for each `Change` variant → implement → green → refactor.
**Abstraction check:** no abstraction on first occurrence of renderer — plain `impl Display` is enough.

---

### Story A-4 · `drift check` CLI Command

> **Persona:** Platform engineer
> **Value:** So that I can run a single command against two specs and get a coloured table on stdout
> **Priority:** P0
> **Size:** S
> **Dependencies:** A-3

**Acceptance Criteria**

```gherkin
Given drift check --base old.yaml --head new.yaml
When one breaking change is found
Then stdout shows a red row with the field path, kind, and severity
And exit code is 1

When no changes are found
Then exit code is 0

When --json flag is passed
Then stdout is valid JSON matching the Change schema
```

**Tasks**

| ID | Hat | Goal | Agent tier | Token budget |
|---|---|---|---|---|
| A-4-T1 | FEATURE | `drift check` subcommand with `--base`, `--head`, `--spec`, `--format`, `--json`, `--no-color` flags | Sonnet | ≤ 2 000 |
| A-4-T2 | FEATURE | Terminal colour rendering: breaking=red, non-breaking-risky=amber, safe=teal, headers=cobalt; respects `NO_COLOR` | Sonnet | ≤ 1 500 |
| A-4-T3 | FEATURE | Exit code semantics: 0=clean, 1=breaking changes found, 2=parse/config error | Sonnet | ≤ 500 |

**CLI output contract:**
```
drift check — API Contract Radar Monitor
════════════════════════════════════════
  BREAKING   user.phone         field_removed
  BREAKING   user.address.zip   type_changed  string→integer
  ok         user.nickname      field_added

2 breaking changes · 1 addition · exit 1
```

---

### Story A-5 · Policy File (`.radar.yml`)

> **Persona:** Platform engineer configuring CI behaviour per repo
> **Value:** So that teams can choose warn-only vs block without changing the CLI invocation
> **Priority:** P0
> **Size:** S
> **Dependencies:** A-4

**Acceptance Criteria**

```gherkin
Given .radar.yml sets block_on: never
When breaking changes are found
Then exit code is 0 (warn only)

Given .radar.yml sets block_on: active_consumers
When no consumers are registered
Then exit code is 0 (no active consumers known yet)

Given .radar.yml is malformed YAML
Then exit code is 2 with a clear error message
```

**Tasks**

| ID | Hat | Goal | Agent tier | Token budget |
|---|---|---|---|---|
| A-5-T1 | FEATURE | Parse `.radar.yml` config; default values when file absent | Sonnet | ≤ 1 500 |
| A-5-T2 | FEATURE | Policy evaluation: `block_on: never | any_break | active_consumers`; exit code decision | Sonnet | ≤ 1 000 |

---

### Story A-6 · GitHub PR Comment

> **Persona:** API producer opening a PR
> **Value:** So that the reviewer sees the breaking changes inline without leaving GitHub
> **Priority:** P0
> **Size:** M
> **Dependencies:** A-4, A-5

**Acceptance Criteria**

```gherkin
Given --post-comment flag and GITHUB_TOKEN in env
When drift check finds breaking changes
Then a comment is posted to the current PR containing a Markdown table of changes

Given the same PR already has a drift comment
When drift check runs again
Then the existing comment is updated, not duplicated
```

**Tasks**

| ID | Hat | Goal | Agent tier | Token budget |
|---|---|---|---|---|
| A-6-T1 | FEATURE | Detect current PR number from `GITHUB_SHA` / `GITHUB_REF` env vars | Sonnet | ≤ 1 000 |
| A-6-T2 | FEATURE | POST/PATCH GitHub comment via REST API; idempotent (find-then-update) | Sonnet | ≤ 2 000 |
| A-6-T3 | FEATURE | Markdown comment template: summary header, changes table, blast-radius placeholder (empty in P0), policy verdict | Sonnet | ≤ 1 500 |

**Security note:** `GITHUB_TOKEN` read from env only; never logged or included in radar-api payloads.

---

### Story A-8 · `radar-ui` + `radar-desktop` Electron Shell (SQLite mode)

> **Persona:** Internal engineer running the tool on their laptop for the first time
> **Value:** So that I can open a desktop app, point it at a spec, and see diffs without configuring any infrastructure
> **Priority:** P0 (tracer bullet requires end-to-end, including UI)
> **Size:** M
> **Dependencies:** A-1 (workspace skeleton)
> **DoR status:** READY after ADR-007, ADR-008, ADR-009 recorded

**Acceptance Criteria**

```gherkin
Given radar-desktop is launched on Windows or macOS
When it starts
Then radar-api sidecar is spawned automatically, pointing at a local SQLite file
And the radar-ui interface loads inside the Electron window

Given the user clicks "Run Check" and selects two spec files
When drift check completes
Then the Diff result appears in radar-ui without opening a terminal

Given the app is closed
Then the radar-api sidecar process is also terminated cleanly

Given radar-api is also accessible via HTTP on localhost during the session
Then radar-cli can connect to it for CI runs targeting the same local data
```

**Tasks**

| ID | Hat | Goal | Agent tier | Token budget |
|---|---|---|---|---|
| A-8-T1 | PREPARATORY | `radar-ui` pnpm workspace with Vite 6 + React 19 + TypeScript + Tailwind + shadcn/ui scaffold; `radar-desktop` pnpm workspace with electron-vite; shared `radar-ui` renderer | Sonnet | ≤ 2 500 |
| A-8-T2 | FEATURE | `radar-api` SQLite mode: `--db sqlite:PATH` flag; sqlx `AnyDatabase` feature; same migrations run on SQLite | Sonnet | ≤ 2 500 |
| A-8-T3 | FEATURE | Electron main process: spawn `radar-api` child process with SQLite path in `userData`; wait for health-check before opening window; terminate on app quit | Sonnet | ≤ 2 500 |
| A-8-T4 | FEATURE | Minimal `radar-ui` home screen: service list (empty state), "Run Check" button that calls radar-api via fetch; displays raw JSON result | Sonnet | ≤ 2 000 |
| A-8-T5 | FEATURE | electron-builder config: Windows NSIS installer, macOS DMG, Linux AppImage; GitHub Actions release job | Sonnet | ≤ 1 500 |

**Security note:** Electron `contextIsolation: true`, `nodeIntegration: false`. All Node.js access via `contextBridge` preload script. radar-api sidecar bound to `127.0.0.1` only — not exposed on the network.

**Hand-off artifact:** Updated Architecture Memory confirming IPC/HTTP boundary between radar-desktop and radar-api.

---

### Story A-7 · `radar-api` Stub — Diff Submission

> **Persona:** CI pipeline
> **Value:** So that diff results are persisted for future dashboard and blast-radius use
> **Priority:** P0
> **Size:** S
> **Dependencies:** A-1 (service skeleton)

**Acceptance Criteria**

```gherkin
Given POST /v1/services/:id/diffs with a valid diff payload
When the request is authenticated with a service token
Then 201 is returned and the diff is stored

Given an invalid payload
Then 422 is returned with structured errors
```

**Tasks**

| ID | Hat | Goal | Agent tier | Token budget |
|---|---|---|---|---|
| A-7-T1 | FEATURE | axum route `POST /v1/services/:id/diffs`; `service`, `diff`, `change` tables via sqlx | Sonnet | ≤ 2 500 |
| A-7-T2 | FEATURE | Service token auth middleware (bearer token, env-seeded for P0) | Sonnet | ≤ 1 500 |
| A-7-T3 | FEATURE | `GET /v1/services/:id/diffs` list endpoint | Sonnet | ≤ 1 000 |

---

### EPIC A — Phase Gate Checklist

- [ ] `drift check` runs green in CI on one real producer repo
- [ ] PR comment posted and updated idempotently
- [ ] Policy file respected (block vs warn)
- [ ] All tasks DoD-passed: 80 % coverage · lint clean · no secrets · no two-hat violations
- [ ] Contract Snapshots written for A-3 public interface
- [ ] Phase Summary + updated Architecture Memory written
- [ ] EPIC B DoR verified before pulling first B story

---

## EPIC B — Consumer Blast Radius & Release Notes

> **Mode:** DELIVERY
> **Theme:** Consumer Registry + Usage Telemetry Ingest + Blast Radius on OpenAPI + Release Notes CLI
> **Tracer bullet:** NO — builds on EPIC A end-to-end path
> **Business value:** Closes the loop — PR comment now names which services break and how recently they called the affected field.
> **Risk:** Consumers don't emit telemetry → tree-sitter static scan (P2) provides baseline; for P1 warn gracefully when usage data is absent.
> **SLO:** Blast radius query p95 < 300 ms; usage ingest p99 < 100 ms
> **Exit criteria:**
> - Three consumers registered; PR comment names them with last-seen timestamps
> - `drift explain --release-notes` outputs valid Markdown from a real Diff
> - Policy `block_on: active_consumers` blocks CI correctly when active consumers exist

---

### Story B-1 · Consumer Registry API

> **Persona:** Consumer team self-registering
> **Value:** So that I declare my dependency on a producer once and get notified of future breaks automatically
> **Priority:** P0
> **Size:** M
> **Dependencies:** A-7 (service + diff tables exist)
> **DoR status:** READY

**Acceptance Criteria**

```gherkin
Given POST /v1/consumers with name, repo_url, owner_team, contact
Then 201 and consumer_id returned

Given POST /v1/services/:id/subscriptions with consumer_id
Then consumer is subscribed to producer

Given GET /v1/services/:id/consumers
Then list of subscribed consumers returned with name and contact
```

**Tasks**

| ID | Hat | Goal | Agent tier | Token budget |
|---|---|---|---|---|
| B-1-T1 | FEATURE | `consumer` + `subscription` tables, migrations | Sonnet | ≤ 1 500 |
| B-1-T2 | FEATURE | `POST /v1/consumers`, `POST /v1/services/:id/subscriptions`, `GET /v1/services/:id/consumers` routes | Sonnet | ≤ 2 500 |
| B-1-T3 | FEATURE | `drift register` CLI subcommand: reads `.radar.yml`, calls POST /v1/consumers + subscription | Sonnet | ≤ 2 000 |

---

### Story B-2 · Usage Event Ingest

> **Persona:** Consumer app emitting OTLP telemetry
> **Value:** So that the system knows which fields each consumer actually calls, not just which ones appear in their code
> **Priority:** P0
> **Size:** M
> **Dependencies:** B-1

**Acceptance Criteria**

```gherkin
Given POST /v1/usage/events with consumer_id, service_id, operation, field_path, ts
Then event is stored and 202 returned

Given 1000 events posted in 1 s
Then p99 latency < 100 ms (rate-limit headers returned at 500 rps)

Given events older than 90 days
When the retention job runs
Then they are deleted
```

**Tasks**

| ID | Hat | Goal | Agent tier | Token budget |
|---|---|---|---|---|
| B-2-T1 | FEATURE | `usage_event` table (hypertable-ready schema); batch insert via `COPY` for throughput | Sonnet | ≤ 2 000 |
| B-2-T2 | FEATURE | `POST /v1/usage/events` — accepts array; rate-limit per service token | Sonnet | ≤ 2 000 |
| B-2-T3 | FEATURE | Retention cron job: delete events older than `lookback_days` × 3 | Sonnet | ≤ 1 000 |

---

### Story B-3 · Blast Radius Computation

> **Persona:** API producer reviewing a PR
> **Value:** So that I see exactly which consumers called the field I'm removing, and when
> **Priority:** P0
> **Size:** L
> **Dependencies:** B-1, B-2

**Acceptance Criteria**

```gherkin
Given a Diff with a Breaking Change on field "user.phone"
And consumer "billing-svc" emitted a usage event for "user.phone" 3 days ago
When GET /v1/diffs/:id/blast-radius is called
Then response includes billing-svc with last_seen=3d and confidence=high

Given no usage events for a field
Then blast radius is empty (not an error)

Given field_path matches a Usage Event within the lookback window
Then consumer is marked "active"; outside window → "stale"
```

**Tasks**

| ID | Hat | Goal | Agent tier | Token budget |
|---|---|---|---|---|
| B-3-T1 | FEATURE | `blast_radius(diff_id) -> Vec<(Consumer, Confidence, LastSeen)>` query | Sonnet | ≤ 2 500 |
| B-3-T2 | FEATURE | `GET /v1/diffs/:id/blast-radius` endpoint | Sonnet | ≤ 1 500 |
| B-3-T3 | FEATURE | Update `drift check` PR comment to include blast radius table (consumer · last-call · severity) | Sonnet | ≤ 2 000 |
| B-3-T4 | FEATURE | Policy `block_on: active_consumers` — evaluate blast radius and set exit code | Sonnet | ≤ 1 000 |

**Contract snapshot:**
```rust
pub struct BlastEntry { pub consumer: Consumer, pub confidence: Confidence, pub last_seen: DateTime<Utc> }
pub async fn blast_radius(diff_id: Uuid, db: &Pool) -> Vec<BlastEntry>
```

---

### Story B-4 · Release Notes Generator

> **Persona:** API producer preparing a release
> **Value:** So that I can paste one command's output into GitHub Releases and consumers know exactly what changed and what to do
> **Priority:** P1
> **Size:** M
> **Dependencies:** B-3 (blast radius enriches the per-consumer section)
> **Feature flag:** `RADAR_RELEASE_NOTES_ENABLED=true` (Claude call gated)

**Acceptance Criteria**

```gherkin
Given drift explain --release-notes --diff-id <id>
Then Markdown output matches the template in SOLUTION_DESIGN §7.4
And breaking changes section lists affected consumers from blast radius
And Claude fills the narrative sections (non-deterministic content is in dedicated blocks)

Given ANTHROPIC_API_KEY is not set
Then narrative sections contain "[narrative unavailable — set ANTHROPIC_API_KEY]" placeholder
And all structured sections still render correctly

Given --out release-notes.md flag
Then the file is written; stdout is silent
```

**Tasks**

| ID | Hat | Goal | Agent tier | Token budget |
|---|---|---|---|---|
| B-4-T1 | FEATURE | Fetch Diff + Blast Radius from radar-api; populate template structured sections deterministically | Sonnet | ≤ 2 500 |
| B-4-T2 | FEATURE | Claude API call for narrative sections (breaking changes plain-language + per-consumer one-liner); prompt-cached | **Opus** | ≤ 3 000 |
| B-4-T3 | FEATURE | `--out FILE` and `--post-github-release` output modes | Sonnet | ≤ 1 500 |

**Agent note:** B-4-T2 uses Opus for highest-quality migration prose. Narrative is clearly demarcated in output so reviewers can verify it before publishing.

---

### EPIC B — Phase Gate Checklist

- [ ] Three consumers registered via `drift register`
- [ ] PR comment names consumers with last-seen timestamps and confidence
- [ ] `block_on: active_consumers` tested end-to-end (blocks when active consumer affected)
- [ ] `drift explain --release-notes` produces valid Markdown from a real Diff
- [ ] All tasks DoD-passed
- [ ] Architecture Memory updated with B-component interfaces
- [ ] EPIC C + D DoR verified before pull

---

## EPIC C — Multi-format + Dashboard + Playground

> **Mode:** DELIVERY
> **Theme:** GraphQL + protobuf parsers; tree-sitter static call-site scanner; drift-dashboard v1; Interactive Playground (Scalar); pre-sales sandbox environment
> **Tracer bullet:** NO
> **Unlock condition:** EPIC A phase gate passed
> **Exit criteria:**
> - `drift check` works on GraphQL SDL and protobuf inputs
> - radar-ui (full dashboard) shows cross-service trend view in both browser and Electron
> - Playground tab shows "Try It" for any registered producer's spec; sandbox environment pre-configured
> - PostgreSQL mode verified: same migrations, same API behaviour as SQLite mode
> - Web self-host confirmed: `docker compose up` brings up radar-api + PostgreSQL + radar-ui in browser

**Stories (DoR to be completed before pull)**

| ID | Title | Size | Agent tier | Dependencies |
|---|---|---|---|---|
| C-1 | Spike — GraphQL schema diff library | S | Opus | — |
| C-2 | Spike — protobuf / buf diff approach | S | Opus | — |
| C-3 | GraphQL SDL parser + Diff | M | Sonnet | C-1 |
| C-4 | Protobuf proto3 parser + Diff | M | Sonnet | C-2 |
| C-5 | tree-sitter consumer repo scanner (TS/Python/Go) | L | Sonnet | B-1 |
| C-6 | tree-sitter Rust + Java grammars | M | Sonnet | C-5 |
| C-7 | `call_site` table + scanner job (cron, not per-PR) | M | Sonnet | C-5 |
| C-8 | Blast radius: union usage events + call sites | S | Sonnet | B-3, C-7 |
| C-9 | radar-ui full shell (sg-shell, sg-nav, dark theme, React Router) | M | Sonnet | A-8 |
| C-10 | radar-ui: Diffs list + Diff detail with blast radius table | M | Sonnet | C-9 |
| C-11 | radar-ui: KPI cards (breaking-changes-30d, consumers-at-risk) | S | Sonnet | C-10 |
| C-12 | Scalar Playground integration (service detail tab) — works in browser and Electron | M | Sonnet | C-9 |
| C-13 | Sandbox environment config (pre-sales base URL + auth injection) | S | Sonnet | C-12 |
| C-14 | PostgreSQL mode: radar-api `--db postgres://…` flag; Docker Compose for web self-host; migration parity test | M | Sonnet | A-8-T2 |
| C-15 | Web deployment: radar-api serves Vite static bundle from `/app`; nginx reverse proxy config | S | Sonnet | C-14 |
| C-16 | Design system token audit across radar-ui + Electron window chrome (§6 compliance) | S | Haiku | C-9–C-15 |

---

## EPIC D — Hardening, Policy Engine, SaaS-viable Deploy

> **Mode:** HARDENING
> **Theme:** Migration guide generator; full policy engine; multi-org scale; GitHub Release automation; performance tests; security review; runbook
> **Tracer bullet:** NO
> **Unlock condition:** EPIC B phase gate passed; EPIC C exit criteria met
> **No new features in HARDENING — only completion, verification, and documentation**

**Stories (DoR to be completed before pull)**

| ID | Title | Size | Agent tier | Dependencies |
|---|---|---|---|---|
| D-1 | Migration guide generator (per-consumer, Claude prose) | M | Opus | B-4 |
| D-2 | Full policy engine (`allow_override_with: label:drift-ack`) | M | Sonnet | A-5 |
| D-3 | `--post-github-release` automation for release notes | S | Sonnet | B-4 |
| D-4 | Multi-org: OIDC dashboard auth; org-scoped service tokens | M | Sonnet | A-7 |
| D-5 | TimescaleDB hypertable migration for `usage_event` (opt-in) | S | Sonnet | B-2 |
| D-6 | Performance test suite: `check` p95 < 5 s, blast-radius p95 < 300 ms | M | Haiku | All |
| D-7 | Security review: threat model (§9) verification, rate limits, token audit | M | Opus | All |
| D-8 | End-to-end smoke test automation (Playwright on dashboard) | M | Sonnet | C-9+ |
| D-9 | Runbook: deploy, rollback, on-call procedures | S | Sonnet | All |
| D-10 | Public OpenAPI spec at `docs/openapi.yaml` | S | Haiku | A-7, B-1–B-3 |
| D-11 | SBOM (syft), cosign-signed release binaries, cargo audit | S | Haiku | All |

---

## EPIC E — Durable Evidence & Differentiator Hardening

> **Mode:** DELIVERY
> **Theme:** Normalize blast-radius evidence into append-only `impact_evidence` records; harden CLI fail-mode semantics; advance scanner to S2; prove the full differentiator flow with fixture-driven demo scenario
> **Tracer bullet:** NO — builds on EPICs A–D end-to-end path
> **Business value:** Closes the evidence gap — blast radius is now backed by durable, explainable, expiry-aware Evidence records. PR comment shows exactly what is known and why CI is blocking.
> **Risk:** tree-sitter TypeScript generated-client detection may require tuning per framework; scope to one major client generator (openapi-typescript-codegen) for E-5.
> **SLO:** blast-radius p95 < 2 s; evidence ingest p99 < 200 ms
> **Exit criteria:**
> - E2E demo scenario test passes: "field removed → billing-svc (high, runtime) + mobile-gateway (low, static) → block"
> - No blast-radius entry returned without at least one `impact_evidence` record
> - CLI fail-open / fail-closed / warn behavior is explicit, tested, and written to Policy Decision
> - PR comment evidence table renders correctly

---

### Story E-1 · `impact_evidence` Table and Blast-Radius Normalization

> **Persona:** Platform engineer reviewing a blast-radius response
> **Value:** So that every consumer listed in blast radius has at least one traceable, timestamped Evidence record I can inspect rather than a recomputed approximation
> **Priority:** P0 (blocks E-4, E-5, E-6)
> **Size:** L
> **Dependencies:** B-3 (blast radius exists), D-4 (org isolation)
> **DoR status:** READY
> **Status: DONE — 2026-05-22**

**Acceptance Criteria**

```gherkin
Given a Diff with a Breaking Change on field "response.user.phone"
And blast_radius() is called for that diff
When GET /v1/diffs/:id/blast-radius is called
Then every consumer entry in the response has at least one evidence record
And evidence records are ordered by confidence descending (high → medium → low)

Given an evidence record with expires_at set to 5 days ago
When GET /v1/diffs/:id/blast-radius?max_age_days=7 is called
Then that stale evidence record is excluded from the response

Given blast_radius() runs for the first time on a diff
Then new impact_evidence rows are inserted (not updated)
```

**Tasks**

| ID | Hat | Goal | Agent tier | Token budget |
|---|---|---|---|---|
| E-1-T1 | PREPARATORY | Migration `007_impact_evidence.sql` — `impact_evidence` table with all fields from data model (id, org_id, diff_id, change_id, producer_service_id, consumer_id, source_type, operation, field_path, confidence, evidence_uri, file_path, line_number, observed_at, expires_at, metadata_json) | Sonnet | ≤ 2 000 |
| E-1-T2 | FEATURE | `blast_radius()` writer — produces `impact_evidence` rows during computation; source_type=runtime_usage or static_call_site based on evidence source | Sonnet | ≤ 3 000 |
| E-1-T3 | FEATURE | `GET /v1/diffs/:id/blast-radius` reader — reads from `impact_evidence` rather than recomputing ad hoc; supports `?max_age_days=` query param for stale exclusion | Sonnet | ≤ 2 500 |
| E-1-T4 | FEATURE | Evidence expiry job — scheduled task that deletes `impact_evidence` rows past `expires_at`; respects org-level retention policy | Sonnet | ≤ 1 500 |

**Contract snapshot (public interface after this story):**
```rust
pub struct Evidence {
    pub id: Uuid,
    pub diff_id: Uuid,
    pub change_id: Uuid,
    pub consumer_id: Uuid,
    pub source_type: EvidenceSourceType,
    pub operation: Option<String>,
    pub field_path: String,
    pub confidence: Confidence,
    pub observed_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
}
pub async fn blast_radius_with_evidence(diff_id: Uuid, max_age_days: Option<u32>, db: &Pool) -> Vec<(Consumer, Vec<Evidence>)>
```

**TDD order:** write failing test asserting evidence row count ≥ 1 per blast-radius consumer → implement writer → green → implement reader → test stale exclusion → green → refactor.

---

### Story E-2 · Org-Scoped Authorization Audit

> **Persona:** Security engineer validating the multi-tenant isolation claims
> **Value:** So that no organization can read, enumerate, or modify another organization's data through any API endpoint
> **Priority:** P0
> **Size:** M
> **Dependencies:** D-4 (org isolation foundation)
> **DoR status:** READY
> **Status: DONE — 2026-05-22**

**Acceptance Criteria**

```gherkin
Given a request authenticated as org A
When GET /v1/diffs/:id is called with a diff_id that belongs to org B
Then 403 Forbidden is returned

Given a request authenticated as org A
When GET /v1/services/:id/consumers is called with a service_id that belongs to org B
Then 403 Forbidden is returned

Given org A's token
When any endpoint listed in SOLUTION_DESIGN §9.2 is called with an org B resource ID
Then 403 is returned — not 200, not 404
```

_All the following resource types must be covered: diffs, spec_versions, usage_events, call_sites, test_suites, release_notes, policies, acknowledgements._

**Tasks**

| ID | Hat | Goal | Agent tier | Token budget |
|---|---|---|---|---|
| E-2-T1 | FEATURE | Integration test suite: for each resource type, assert org A token returns 403 on org B resource IDs; parameterised over all endpoint groups | Sonnet | ≤ 3 000 |
| E-2-T2 | FEATURE | Fix any org_id enforcement gaps discovered by E-2-T1; update middleware or query filters as needed | Sonnet | ≤ 2 500 |

**TDD order:** write the full 403-assertion test matrix first (all red) → fix gaps → all green → refactor shared test helpers.

---

### Story E-3 · CLI Fail-Mode Hardening

> **Persona:** Platform engineer configuring CI gating behaviour for a producer repo
> **Value:** So that I can explicitly choose how the CI gate behaves when Radar API is unreachable, and that choice is recorded in a durable Policy Decision rather than silently falling through
> **Priority:** P0
> **Size:** S
> **Dependencies:** A-5 (policy file), D-2 (policy engine)
> **DoR status:** READY
> **Status: DONE — 2026-05-22**

**Acceptance Criteria**

```gherkin
Given .radar.yml sets fail_mode: warn
When drift check runs with a breaking change
Then exit code is 0 and a warning line is printed
And a Policy Decision record is written with verdict=warn and fail_mode=warn

Given .radar.yml sets fail_mode: open
And the Radar API is unreachable
When drift check runs
Then the structural diff still runs locally
And exit code reflects only local diff result (1 if breaking, 0 if clean)
And a Policy Decision record is written with verdict=warn and fail_mode=open

Given .radar.yml sets fail_mode: closed
And the Radar API returns 500
When drift check runs
Then exit code is 1 (blocked)
And a Policy Decision record is written with verdict=block and fail_mode=closed

Given .radar.yml omits fail_mode
Then fail_mode defaults to closed
```

**Tasks**

| ID | Hat | Goal | Agent tier | Token budget |
|---|---|---|---|---|
| E-3-T1 | FEATURE | `fail_mode` field in `.radar.yml` config struct; parsing, validation, default=closed; all three mode behaviors wired into policy evaluation | Sonnet | ≤ 2 000 |
| E-3-T2 | FEATURE | Policy Decision persistence — `POST /v1/policy-decisions` endpoint; `drift check` writes a Policy Decision record after every run | Sonnet | ≤ 2 000 |

**TDD order:** write three failing tests (one per mode) before implementing fail-mode parsing; write Policy Decision persistence test before implementing the API call.

---

### Story E-4 · PR Comment Evidence Rendering

> **Persona:** API producer and reviewer reading a PR comment
> **Value:** So that I can see exactly which consumers are at risk, why Radar believes that, and what the policy verdict means — all without leaving GitHub
> **Priority:** P1
> **Size:** M
> **Dependencies:** E-1 (evidence records exist), E-3 (policy verdict record exists)
> **DoR status:** READY after E-1, E-3 complete
> **Status: DONE — 2026-05-25**

**Acceptance Criteria**

```gherkin
Given a Diff with blast radius containing one high-confidence runtime evidence entry and one low-confidence static entry
When the PR comment is rendered
Then it contains an evidence table section with columns: Consumer | Source | Operation | Field Path | Confidence | Last Seen
And the high-confidence row appears before the low-confidence row
And it contains a policy verdict section with: verdict badge, fail_mode, required action text, and override instruction

Given the Policy Decision verdict is block
Then the verdict badge reads "BLOCKED" and the required action text is non-empty

Given the Policy Decision verdict is warn
Then the verdict badge reads "WARNED" and no blocking action is required
```

**Tasks**

| ID | Hat | Goal | Agent tier | Token budget |
|---|---|---|---|---|
| E-4-T1 | FEATURE | Evidence table renderer — Markdown table section; sorts by confidence descending; truncates at 10 rows with "N more…" footer | Sonnet | ≤ 2 000 |
| E-4-T2 | FEATURE | Policy verdict section renderer — verdict badge (BLOCKED / WARNED / PASSED / OVERRIDDEN), fail_mode label, required action text, override instruction block | Sonnet | ≤ 1 500 |
| E-4-T3 | FEATURE | Update `drift check` PR comment to include evidence table and policy verdict sections after the existing blast-radius table | Sonnet | ≤ 1 500 |

**PR comment evidence table template:**
```
### Evidence

| Consumer | Source | Operation | Field Path | Confidence | Last Seen |
|---|---|---|---|---|---|
| billing-svc | runtime_usage | GET /users/{id} | response.user.phone | high | 2 days ago |
| mobile-gateway | static_call_site | — | response.user.phone | low | (static) |
```

**PR comment policy verdict template:**
```
### Policy Verdict

> **BLOCKED** · fail_mode: closed

2 consumers affected. At least 1 high-confidence evidence record present.
To override: add the `drift-ack` label to this PR and re-run CI.
```

---

### Story E-5 · Operation-Aware TypeScript Scanner (S2)

> **Persona:** Static scanner advancing from field-only (S1) to operation-correlated (S2) for TypeScript
> **Value:** So that blast-radius confidence reflects whether Radar knows which API operation a call site is targeting, reducing false positives and giving Policy a signal to act on
> **Priority:** P1
> **Size:** L
> **Dependencies:** C-5 (tree-sitter scanner exists at S1), E-1 (confidence field in impact_evidence)
> **DoR status:** READY after E-1 complete
> **Status: DONE — 2026-05-25**

**Acceptance Criteria**

```gherkin
Given a TypeScript fixture file that calls a generated client method `usersApi.getUserById(id)`
And the generated client is known to map to operation "GET /users/{id}"
When the scanner processes the fixture
Then a call_site row is written with operation="GET /users/{id}" and field_path="response.user.phone"
And confidence is medium (operation known, field extracted from response)

Given a TypeScript fixture file that reads `response.user.phone` with no known generated client context
When the scanner processes the fixture
Then a call_site row is written with operation=NULL and confidence=low

Given an impact_evidence record seeded from a low-confidence static call_site (operation=NULL)
When policy is set to ignore_low_confidence_static=true
Then that evidence record does not contribute to a block verdict
```

**Tasks**

| ID | Hat | Goal | Agent tier | Token budget |
|---|---|---|---|---|
| E-5-T1 | FEATURE | TypeScript generated-client detection — tree-sitter query identifies `new ApiClient()` patterns and method calls matching known generated-client shapes (openapi-typescript-codegen, orval); emits (method_name → operation) mapping | Sonnet | ≤ 3 500 |
| E-5-T2 | FEATURE | Operation correlation logic — resolves detected method names against service spec operations; writes `operation` column in `call_site`; marks unresolved as NULL | Sonnet | ≤ 2 500 |
| E-5-T3 | FEATURE | Confidence propagation — when `impact_evidence` is seeded from a `call_site`, set confidence=medium if operation is populated, confidence=low if operation is NULL | Sonnet | ≤ 1 500 |

**TDD order:** write fixture TypeScript file with known generated client call → write failing test asserting operation column populated → implement detection → green → write low-confidence fixture test → green → refactor.

---

### Story E-6 · Demo Scenario Fixtures

> **Persona:** Architect and developer validating the full differentiator claim end-to-end
> **Value:** So that we can prove the "field removed → evidence → block" flow in a single repeatable test without requiring live external services
> **Priority:** P1
> **Size:** M
> **Dependencies:** E-1, E-3, E-4, E-5
> **DoR status:** READY after E-1, E-3, E-4, E-5 complete
> **Status: DONE — 2026-05-25**

**Acceptance Criteria**

```gherkin
Given the demo fixtures are loaded (specs, usage event, call site)
When cargo test --test demo_scenario is run
Then the test passes green

Given demo-payments-api v1 has field response.user.phone
And demo-payments-api v2 removes that field
When diff is computed
Then a Breaking Change of kind=field_removed on path=response.user.phone is produced

Given billing-svc usage event fixture (response.user.phone observed 2 days ago)
When blast radius is computed
Then billing-svc appears with confidence=high and source_type=runtime_usage

Given mobile-gateway static call-site fixture (TypeScript generated client call)
When blast radius is computed
Then mobile-gateway appears with confidence=low and source_type=static_call_site

Given the blast radius contains both consumers
When the PR comment is rendered from fixture
Then it exactly matches the expected PR comment fixture file
```

**Tasks**

| ID | Hat | Goal | Agent tier | Token budget |
|---|---|---|---|---|
| E-6-T1 | PREPARATORY | Create fixture directories: `fixtures/demo-payments-api/` (v1.yaml, v2.yaml), `fixtures/demo-billing-svc/` (usage_events.json), `fixtures/demo-mobile-gateway/` (src/clients/users.ts generated client call) | Sonnet | ≤ 2 000 |
| E-6-T2 | FEATURE | Expected PR comment fixture file `fixtures/expected-pr-comment.md`; deterministic enough for byte-level assertion on structured sections | Sonnet | ≤ 1 500 |
| E-6-T3 | FEATURE | Integration test `tests/demo_scenario.rs`: loads fixtures, seeds DB, runs diff, blast radius, PR comment render; asserts all fixture expectations | Sonnet | ≤ 3 000 |

**Hand-off artifact:** `fixtures/README.md` explaining the demo scenario, how to load fixtures locally, and what each fixture proves.

---

### Story E-7 · Collection File Scanner — Postman Collection v2.1 as Consumer Evidence Source

> **Persona:** Platform engineer or Consumer team lead who maintains API test collections (Postman, Insomnia, NativeREST) in source control
> **Value:** So that Radar automatically derives Consumer evidence from committed collection files without requiring instrumented runtime telemetry or tree-sitter code analysis
> **Priority:** P1
> **Size:** M
> **Dependencies:** E-1 (impact_evidence schema), E-5 (S2 scanner architecture)
> **INVEST:** Independent · Negotiable · Valuable · Estimable · Small · Testable
> **DoR status:** READY after E-1 complete
> **Status: DONE — 2026-05-25**

**Acceptance Criteria**

```gherkin
Given a Consumer repo contains `collections/payments.postman_collection.json` (Postman Collection v2.1)
And the collection includes a request "GET /users/{id}" with test assertions checking "response.user.phone"
When the scanner processes the file
Then a call_site row is written with source_type=collection_file, operation="GET /users/{id}", field_path="response.user.phone", confidence=medium

Given the collection contains only a request "POST /orders" with no test assertions
When the scanner processes the file
Then a call_site row is written with source_type=collection_file, operation="POST /orders", field_path=NULL, confidence=medium

Given the collection's "info.name" is "Billing Service Tests"
And no existing Consumer row matches that name for this producer
When the scanner processes the file
Then a new Consumer is registered with name="Billing Service Tests" and catalog_source=collection_file
And subsequent blast-radius computation includes this Consumer

Given a collection file with a variable base URL "{{base_url}}/users/{id}"
When the scanner processes the file
Then the variable prefix is stripped and operation is resolved to "/users/{id}"

Given the scanner processes the same collection file twice without changes between runs
When impact_evidence is written
Then no duplicate evidence rows are inserted (idempotent on file hash + consumer_id + operation + field_path)

Given a malformed or non-v2.1 JSON file at the configured path
When the scanner processes the file
Then it logs a structured warning with file path and error reason and continues without panicking
```

**Tasks**

| ID | Hat | Goal | Agent tier | Token budget |
|---|---|---|---|---|
| E-7-T1 | PREPARATORY | `CollectionFile` Rust struct + `parse_collection(path) -> Result<Vec<CollectionRequest>>` — deserialise Postman Collection v2.1 JSON; extract `info.name`, request items, URL template, method, and `event[].script.exec` test scripts; strip `{{variable}}` prefixes from URLs | Sonnet | ≤ 3 000 |
| E-7-T2 | FEATURE | Field-path extraction from test scripts — scan `exec` lines for `.json().<field>`, `pm.response.json().<path>`, and `jsonPath` patterns; produce best-effort `field_path` strings (NULL when unresolvable) | Sonnet | ≤ 2 500 |
| E-7-T3 | FEATURE | Consumer auto-registration — on first scan of a collection file, upsert a Consumer row using `info.name` as display name; `catalog_source=collection_file`; idempotent on `(org_id, name)` | Sonnet | ≤ 1 500 |
| E-7-T4 | FEATURE | Evidence writer — for each `CollectionRequest` produce one `impact_evidence` row: `source_type=collection_file`, `confidence=medium` (operation known), `evidence_uri=file://<relative_path>#<request_name>`; dedup on `(diff_id, consumer_id, operation, field_path, source_type)` using INSERT OR IGNORE / ON CONFLICT DO NOTHING | Sonnet | ≤ 1 500 |
| E-7-T5 | FEATURE | Scanner configuration — `collection_paths` glob list in scanner config TOML (e.g. `["**/*.postman_collection.json", "**/*.nativerest_collection.json"]`); scanner walks configured paths and invokes the parser | Sonnet | ≤ 1 000 |

**Contract snapshot (public interface after this story):**
```rust
pub struct CollectionRequest {
    pub name: String,
    pub method: String,
    pub operation: Option<String>,   // normalised path e.g. "/users/{id}"
    pub field_paths: Vec<String>,    // extracted from test scripts; may be empty
}

pub fn parse_collection(path: &Path) -> Result<(String, Vec<CollectionRequest>)>
// returns (collection_name, requests)
```

**New `source_type` enum variant:** `collection_file` (added to `EvidenceSourceType` in radar-core; new migration not required — stored as TEXT)

**Test data:** `fixtures/billing-svc-tests.postman_collection.json` — a minimal v2.1 collection with one `GET /users/{id}` request and one test assertion on `response.user.phone`; one `POST /orders` request with no test assertions; one request with a `{{base_url}}` variable prefix.

**Idempotency strategy:** `INSERT … ON CONFLICT (diff_id, consumer_id, operation, field_path, source_type) DO NOTHING`; file hash stored in `evidence_uri` allows re-scan detection.

**TDD order:** write fixture JSON → write failing test for `parse_collection` extracting `CollectionRequest` list → implement parser → green; write failing test for evidence dedup (run twice, assert row count unchanged) → implement writer → green; refactor.

---

### EPIC E — Phase Gate Checklist

- [x] `cargo test --test demo_scenario` passes green on a clean checkout _(E-6 done — 6 integration tests)_
- [x] No blast-radius API response contains a consumer without at least one `impact_evidence` row _(E-1 done)_
- [x] All three fail-modes (closed, open, warn) tested with Policy Decision record assertions _(E-3 done)_
- [x] PR comment evidence table renders correctly with correct confidence ordering _(E-4 done — 14 new github tests)_
- [x] Scanner S2: TypeScript generated-client call site produces operation-populated `call_site` row _(E-5 done — 12 new scanner tests)_
- [x] Collection File Scanner: Postman Collection v2.1 parsed and evidence rows written with source_type=collection_file _(E-7 done — 9 scanner tests + 4 API tests)_
- [x] Org isolation tests from E-2 all green _(E-2 done — 7 cross-org 403 tests)_
- [ ] All tasks DoD-passed: 80 % coverage · lint clean · no secrets · no two-hat violations
- [ ] Architecture Memory updated (E hand-off)
- [ ] EPIC F DoR verified before pull

---

## EPIC F — Enterprise Workflow Packaging

> **Mode:** DELIVERY
> **Theme:** GitHub Action; policy decisions table; acknowledgement workflow; Backstage and CODEOWNERS catalog importers; dashboard enterprise pages
> **Tracer bullet:** NO
> **Unlock condition:** EPIC E phase gate passed
> **Exit criteria:**
> - New repo can install radar-action from docs in under 15 minutes without custom scripting
> - PR comment clearly explains pass / warn / block with evidence
> - Overrides are recorded in `acknowledgement` table and visible in audit trail

**Stories (DoR to be completed before pull)**

| ID | Title | Size | Agent tier | Dependencies |
|---|---|---|---|---|
| F-1 | `radar-action` — GitHub Action composite action (TypeScript) | L | Sonnet | E-3, E-4 | _(DONE 2026-05-25 — composite action with Rust toolchain + cargo cache + --summary-file bridge)_ |
| F-2 | `policy_decision` table + persistence in radar-api | M | Sonnet | E-3 | _(pre-delivered by E-3-T2 — migration 012, POST /v1/policy-decisions, CLI wiring all done)_ |
| F-3 | Acknowledgement workflow — `acknowledgement` table + API endpoints + UI | L | Sonnet | E-2, F-2 | _(DONE 2026-05-25 — migration 014, POST/GET /v1/acknowledgements + /diffs/:id/acknowledgements, CLI check wired, 4 API tests)_ |
| F-4 | Backstage `catalog-info.yaml` importer (polling job) | M | Sonnet | — | _(DONE 2026-05-25 — migration 015, POST/GET /v1/catalog-sources, POST /v1/catalog-sources/:id/sync, Backstage entity upsert; 3 tests)_ |
| F-5 | CODEOWNERS fallback importer | S | Sonnet | F-4 | _(DONE 2026-05-25 — parse_codeowners() + sync_codeowners_source() wired into sync handler; 3 unit tests)_ |
| F-6 | Catalog sync status in dashboard UI (`catalog_source` table + sync page) | S | Sonnet | F-4 | _(DONE 2026-05-25 — CatalogSourcesPage.tsx; create + sync-now per row; nav entry under Registry)_ |
| F-7 | Acknowledgement workflow in dashboard UI (diff detail page, ack button, override flow) | M | Sonnet | F-3 | _(DONE 2026-05-25 — acknowledgement section on DiffDetailPage; create-ack form; live list after submit)_ |
| F-8 | Audit trail page in dashboard UI (paginated, org-scoped) | S | Sonnet | F-3 | _(DONE 2026-05-25 — AuditPage.tsx; paginated policy decisions + acknowledgements tables; Governance nav section)_ |
| F-9 | Documentation — `docs/getting-started-github-action.md`, `docs/backstage-integration.md`, `docs/policy-reference.md`, `docs/oidc-setup.md` | M | Sonnet | F-1, F-4 | _(DONE 2026-05-25)_ |

---

## EPIC F+ — Evolution Rules

> **Mode:** DELIVERY
> **Theme:** Operator-defined severity overrides per change kind; glob path matching; server-side evaluation in diff response; CLI management; dashboard UI
> **Tracer bullet:** NO
> **Unlock condition:** EPIC F complete
> **Exit criteria:**
> - Rules stored in `evolution_rule` table (migration 016); org-scoped
> - `GET /v1/diffs/:id` applies active rules at query time — severity field overridden, `applied_rule` attached
> - Rules can only downgrade severity (never tighten); first match wins
> - CLI: `radar rule add|list|delete|toggle|test`
> - Dashboard: `/evolution-rules` page with enable/disable toggle and delete

| ID | Title | Size | Done |
|---|---|---|---|
| F+-1 | Migration 016 + evolution_rule CRUD API (POST/GET/DELETE/PATCH) | S | _(DONE 2026-05-25 — 5 new API tests; path_matches + severity_rank helpers)_ |
| F+-2 | Server-side rule evaluator in get_diff — severity override + applied_rule field | M | _(DONE 2026-05-25 — integrated into GET /v1/diffs/:id)_ |
| F+-3 | CLI `radar rule` subcommands (add, list, delete, toggle, test) | S | _(DONE 2026-05-25 — RuleAction enum; api_client functions)_ |
| F+-4 | Dashboard UI — EvolutionRulesPage + Governance nav section | S | _(DONE 2026-05-25 — enable/disable toggle, delete, create form)_ |

---

## EPIC G — Runtime Evidence Collection

> **Mode:** DELIVERY
> **Theme:** OTel collector processor; API gateway adapters; Node/Express and FastAPI middleware SDKs; evidence freshness dashboard; sampling controls; privacy documentation
> **Tracer bullet:** NO
> **Unlock condition:** EPIC E phase gate passed; EPIC F may run in parallel
> **Exit criteria:**
> - At least one real service produces usage Evidence via OTel processor or gateway adapter without custom application code
> - Dashboard shows evidence coverage by service and Consumer
> - Stale evidence is visible and expires predictably

**Stories (DoR to be completed before pull)**

| ID | Title | Size | Agent tier | Dependencies |
|---|---|---|---|---|
| G-1 | Spike — OTel collector processor architecture | S | _(DONE 2026-05-25 — OTLP-over-HTTP in radar-api; no separate Go process required)_ |
| G-2 | OTel collector processor — OTLP JSON trace receiver in radar-api | L | _(DONE 2026-05-25 — POST /v1/otlp/v1/traces; CLIENT span extraction; path normalisation)_ |
| G-3 | API gateway adapter — Kong / NGINX log ingestion | M | _(DONE 2026-05-25 — POST /v1/gateway/logs; numeric segment normalisation)_ |
| G-4 | Node/Express middleware SDK | M | _(DONE 2026-05-25 — @radar-monitor/sdk; RadarBatcher; expressMiddleware; recordFieldUsage; 4 tests)_ |
| G-5 | FastAPI middleware SDK (Python) | M | _(DONE 2026-05-25 — radar-monitor-sdk; RadarBatcher; RadarMiddleware ASGI; 12 tests)_ |
| G-6 | Ingestion sampling controls (per-service sample rate, field-path allow/block list) | S | _(DONE 2026-05-25 — service_sampling table; PUT/GET /v1/services/:id/sampling; field_deny_list glob; probabilistic sample_rate)_ |
| G-7 | Evidence freshness dashboard page (coverage by service and Consumer, stale warning) | M | _(DONE 2026-05-25 — EvidenceCoveragePage; Governance nav; stale row warnings; SDK callout)_ |
| G-8 | Privacy/redaction documentation (`docs/runtime-usage-ingestion.md`, `docs/security-and-privacy.md`) | S | _(DONE 2026-05-25 — both docs written)_ |

---

## EPIC H — Impact-Targeted Artifacts

> **Mode:** DELIVERY
> **Theme:** Diff+evidence-scoped test generation; deterministic templates per change kind; per-Consumer migration guides; release-note state workflow; generated artifacts in PR comment and dashboard
> **Tracer bullet:** NO
> **Unlock condition:** EPIC E phase gate passed
> **Exit criteria:**
> - For each Breaking Change kind in the 5 templates, Radar generates at least one relevant test Artifact from the Diff + Evidence (not requiring a Jira ticket)
> - Release Notes include affected Consumers and Evidence
> - Migration Guide is scoped to Consumer usage (call sites + runtime Evidence)

**Stories (DoR to be completed before pull)**

| ID | Title | Size | Agent tier | Dependencies |
|---|---|---|---|---|
| H-1 | Test generation from diff + evidence context (accepts diff_id, not just Jira/spec) | L | _(DONE 2026-05-25 — diff_id-only path; evidence context; AI or template fallback; migration 019)_ |
| H-2 | Deterministic test templates per change kind (5 templates: field_removed, required_changed, enum_value_removed, operation_removed, type_changed) | M | _(DONE 2026-05-25 — templates_from_changes; 4 unit tests; evidence [evidence] tag)_ |
| H-3 | Per-Consumer migration guides scoped to Consumer usage and call sites | M | _(DONE 2026-05-25 — GET /v1/diffs/:id/migration-guide?consumer_id=; Markdown with change advice, evidence table, call-site table)_ |
| H-4 | Release-note state workflow (draft → reviewed → published → superseded) | M | _(DONE 2026-05-25 — migration 018; PATCH /v1/release-notes/:id/status; state-machine guard; 2 API tests)_ |
| H-5 | Generated test artifacts linked in PR comment | S | _(DONE 2026-05-25 — GET /v1/diffs/:id/test-suites; build_comment_with_suites; fetch_diff_test_suites in api_client)_ |
| H-6 | Artifact review/publish controls in dashboard UI | M | _(DONE 2026-05-25 — ReleaseNotesPage: StatusBadge, transition buttons, state machine, optimistic update)_ |

---

## EPIC I — Public Readiness

> **Mode:** HARDENING
> **Theme:** Polished demo repo; public documentation; self-host install guide; benchmark suite; SBOM and signed binaries; demo video script
> **Tracer bullet:** NO
> **Unlock condition:** EPICs E–H complete (F, G, H may overlap)
> **No new features in HARDENING — only completion, verification, and documentation**
> **Exit criteria:**
> - Public docs state the "impact-aware contract drift" product promise without caveats
> - Demo works from clean clone with `docker compose up`
> - CI is green
> - Enterprise pilot checklist is complete (§3.2 of SOLUTION_DESIGN v1.0)

**Stories (DoR to be completed before pull)**

| ID | Title | Size | Agent tier | Dependencies |
|---|---|---|---|---|
| I-1 | Demo repository set: `fixtures/demo-payments-api/`, `fixtures/demo-billing-svc/`, `fixtures/demo-mobile-gateway/` with seeded runtime usage and GitHub workflow | M | Sonnet | E-6 | **DONE** |
| I-2 | Polished README with installation, demo scenario, and architecture diagram | M | Sonnet | I-1 | **DONE** |
| I-3 | `docs/evidence-confidence.md`, `docs/security-and-privacy.md`, `docs/demo-scenario.md`, `docs/enterprise-deployment.md` | M | Sonnet | G-8 | **DONE** |
| I-4 | Self-host install guide (`docs/enterprise-deployment.md`) — Docker Compose + PostgreSQL + OIDC | S | Sonnet | — | **DONE** (merged into I-3) |
| I-5 | Benchmark suite: `drift check` p95 < 10 s, blast-radius p95 < 2 s, usage ingest p95 < 500 ms | M | Haiku | All | **DONE** (`radar-core/benches/diff_bench.rs`) |
| I-6 | SBOM (syft), cosign-signed release binaries, cargo audit, licensing review | S | Haiku | All | **DONE** (CI: cargo-audit + cargo-cyclonedx; `LICENSE` file added) |
| I-7 | `docs/generated-artifacts.md` and demo video script | S | Sonnet | H-6 | **DONE** |

---

## Agent Capability Assignment Summary

| Work type | Assigned tier | Rationale (Core Spec §6) |
|---|---|---|
| Architecture decisions, ADRs, spike evaluation | **Opus** | Reasoning-heavy; judgment matters more than speed |
| Migration guide + release-notes narrative (Claude calls) | **Opus** | Highest prose quality for consumer-facing content |
| Feature implementation, CLI commands, API routes, UI components | **Sonnet** | Code generation sweet spot; speed + quality |
| Refactoring, preparatory restructuring | **Sonnet** | Needs understanding of existing structure |
| Lint verification, DoD checklist execution, design-system audit | **Haiku** | Fast, cheap, deterministic checks |

---

## Global Definition of Done

_Every task must pass before the story is considered complete._

- [ ] Tests written first (TDD order respected — failing test → implementation → green → refactor)
- [ ] No secrets in code or logs
- [ ] `cargo clippy -- -D warnings` passes / `pnpm lint` passes
- [ ] ≥ 80 % line coverage on new code (`cargo tarpaulin` / `vitest --coverage`)
- [ ] Contract tests written for any new public interface (API route or public Rust fn)
- [ ] Feature flag present if the task introduces a Claude API call or destructive migration
- [ ] Hand-off artifact written (updated Architecture Memory or Contract Snapshot)
- [ ] Domain Glossary consistent — no new terms introduced without updating §Domain Glossary above
- [ ] No duplicated logic — abstraction check passed
- [ ] No undocumented shortcuts — Technical Debt Items filed if deadline pressure forces one

---

## Flow Metrics Targets

| Metric | PROTOTYPE target | DELIVERY target |
|---|---|---|
| Story cycle time (p50) | ≤ 2 days | ≤ 1.5 days |
| Story cycle time (p85) | ≤ 4 days | ≤ 3 days |
| Rework rate (stories returned from DoD) | < 20 % | < 10 % |
| Two-hat violations | 0 | 0 |
| TD interest (TD tasks as % of active work) | < 30 % | < 20 % |

**Escalation triggers (GPM v2.1 §Escalation):**
- Rework > 30 % → Architect reviews prompt quality + DoR rigour
- Cycle time trending up 2 sprints in a row → decompose stories further
- Smoke test failures at phase gate → halt, fix, re-run gate
- Glossary drift detected → freeze new stories until glossary resolved

---

## Changelog

| Version | Date | Author | Change |
|---|---|---|---|
| 0.1 | 2026-05-17 | Yannick Verrydt | Initial development plan — Phase 0, EPIC A (full), EPIC B (full), EPIC C/D (outline); framework: GPM v2.1 + Backlog Builder v5.1 + Core Spec v1 |
| 0.2 | 2026-05-17 | Yannick Verrydt | Electron + Web dual deployment: replaced Next.js with Vite 6; added radar-ui (shared renderer) and radar-desktop (Electron shell); added Story A-8 (Electron shell + SQLite mode); added ADR-007/008/009; added SQLite/PostgreSQL database abstraction (ADR-002 revised); expanded EPIC C with C-14/C-15/C-16; updated Architecture Memory, Glossary, and EPIC exit criteria |
| 0.3 | 2026-05-21 | Yannick Verrydt | Enterprise EPICs E–I added: durable evidence, GitHub Action, OTel ingest, impact-targeted artifacts, public readiness; new glossary terms (Evidence, Confidence, Policy Decision, Acknowledgement, Artifact, Catalog Source, Fail Mode, Scanner Stage, Demo Scenario); ADR-012 through ADR-015; updated Architecture Memory |
| 0.4 | 2026-05-22 | Yannick Verrydt | E-1, E-2, E-3 marked DONE; phase gate checklist updated; F-2 noted as pre-delivered by E-3-T2; next story is E-4 (PR Comment Evidence Rendering) |
| 0.5 | 2026-05-25 | Yannick Verrydt | E-4, E-5, E-6 marked DONE; E-7 story drafted; EPIC E phase gate 5/10 items checked; next is E-7 (Postman Collection Scanner) |
| 0.6 | 2026-05-25 | Yannick Verrydt | E-7 marked DONE; EPIC E phase gate 6/10 items checked; migration 013; 27 scanner tests + 42 API tests; EPIC F is next |
| 0.7 | 2026-05-25 | Yannick Verrydt | Tech debt pass complete (ChangeKind::as_str(), JiraTicket cleanup, README/CLAUDE.md rewrite); F-1 DONE — radar-action composite action with --summary-file bridge; 3 new render tests; 182 workspace tests green |
| 0.8 | 2026-05-25 | Yannick Verrydt | EPIC F complete — F-3 (acknowledgement API + CLI override), F-4 (catalog sources + Backstage importer), F-5 (CODEOWNERS importer), F-6 (CatalogSourcesPage), F-7 (DiffDetailPage ack workflow), F-8 (AuditPage), F-9 (docs); new Governance nav section; 52 API tests, 0 TS errors |
| 0.9 | 2026-05-25 | Yannick Verrydt | EPIC F+ complete — evolution rules (migration 016, CRUD API, server-side evaluator in get_diff, CLI `radar rule`, EvolutionRulesPage); 65 API tests, 211 workspace tests, 0 TS errors |
| 1.0 | 2026-05-25 | Yannick Verrydt | EPIC G complete — OTLP trace receiver, gateway log ingestion, sampling controls (migration 017, field_deny_list glob, probabilistic sample_rate), @radar-monitor/sdk (Node.js), radar-monitor-sdk (Python, ASGI), EvidenceCoveragePage + Governance nav, docs/runtime-usage-ingestion.md, docs/security-and-privacy.md |
| 1.1 | 2026-05-25 | Yannick Verrydt | EPIC H complete — H-1 diff-based test gen (no Jira), H-2 deterministic templates (5 change kinds), H-3 migration guide endpoint, H-4 release-note status workflow (migrations 018/019), H-5 test suites in PR comment, H-6 ReleaseNotesPage status transitions; new github test |
| 1.2 | 2026-05-25 | Yannick Verrydt | EPIC I complete — I-1 demo fixtures + seed-demo.sh + payments-api GitHub workflow; I-2 README architecture diagram + 5-minute demo section; I-3/I-4 docs/evidence-confidence.md + docs/demo-scenario.md + docs/enterprise-deployment.md; I-5 radar-core/benches/diff_bench.rs (Criterion); I-6 LICENSE file (cargo-audit + SBOM already in CI); I-7 docs/generated-artifacts.md + docs/demo-video-script.md; 83 API tests + clippy clean |

# API Contract Drift Monitor — Development Plan

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
| **Desktop Mode** | `drift-api` running with SQLite, spawned as a child process by `drift-desktop` (Electron), with no external infrastructure |
| **Web Mode** | `drift-api` running with PostgreSQL, deployed on a server, with `drift-ui` served as a static bundle |

### Architecture Memory

```
drift-cli        Rust binary — local/CI diff runs, release-notes generation, PR comment posting
drift-api        Rust/axum HTTP service — Consumer Registry, usage ingest, diff result store
                 Compiled once; targets SQLite (local/Electron) or PostgreSQL (web/prod) via --db flag
                 Also serves drift-ui static bundle at /app in web mode
drift-scanner    Rust/tree-sitter background worker — call-site extraction from Consumer repos
drift-ui         Vite 6 + React 19 + Tailwind + shadcn/ui — shared renderer
                 Runs in browser (web) or inside drift-desktop Electron renderer (desktop)
drift-desktop    Electron 33 shell — wraps drift-ui; spawns drift-api sidecar; manages SQLite file
drift-db         SQLite file (local/Electron default) | PostgreSQL 16 (web/production)
                 Same sqlx migrations apply to both; TimescaleDB optional on PostgreSQL at scale

Dependencies point inward: cli → api; drift-ui → api (HTTP); drift-desktop → drift-ui + drift-api (IPC+HTTP); scanner → api; nothing → cli
```

### Architectural Decision Records

| ADR | Decision | Rationale | Alternatives rejected |
|---|---|---|---|
| ADR-001 | Rust for all backend components | Spec parsing is CPU-bound per PR; cross-platform binary ships without a runtime | Go (no tree-sitter bindings as mature), Python (too slow for per-PR parsing) |
| ADR-002 | SQLite (local/Electron default) + PostgreSQL 16 (web/production) via sqlx | sqlx `AnyDatabase` lets `drift-api` compile once and target either engine; SQLite gives zero-setup desktop experience; PostgreSQL unlocks HA and TimescaleDB at scale | Two separate ORMs (more code, drift risk), SQLite-only (no web scale path), PostgreSQL-only (kills offline/desktop use) |
| ADR-003 | Scalar (MIT) for API Playground | Zero per-seat cost; renders directly from parsed OpenAPI spec; self-hosted; skinnable | Postman (paid per-seat), Swagger UI (older UX), Redoc (no interactive testing) |
| ADR-004 | Claude for narrative generation only | Diff classification must be deterministic and auditable; only human-readable prose uses LLM | LLM for classification (non-deterministic, hallucination risk on field paths) |
| ADR-005 | tree-sitter for call-site extraction | Language-agnostic, embeddable in Rust, mature grammars for TS/Python/Go/Rust/Java | Regex (brittle), language-specific AST tools (can't unify in one binary) |
| ADR-006 | OTLP-compatible ingest for Usage Events | Consumers already export OTLP; re-using the same pipeline avoids a second SDK | Custom SDK (adoption friction), log scraping (unstructured) |
| ADR-007 | Vite 6 over Next.js 15 for `drift-ui` | Next.js SSR runs in a Node.js process; Electron's renderer is a Chromium page — SSR is incompatible. Vite produces a static bundle that loads identically in a browser or Electron renderer. In web mode, drift-api serves the Vite build from `/app`. | CRA (deprecated), Next.js with `output: 'export'` (loses API routes, SSR, image optimisation — same result as Vite but with more complexity) |
| ADR-008 | Electron 33 for desktop distribution | Cross-platform installers (.exe / .dmg / .AppImage) with one codebase; main process can spawn drift-api as a child process and manage the SQLite file lifecycle; auto-update via electron-updater | Tauri (Rust-native, lighter, but WebView rendering differs per OS which would require significant CSS testing), PWA (no sidecar process management, no offline DB) |
| ADR-009 | electron-vite as the Electron build tool | Same Vite config for `drift-ui` (web) and `drift-desktop` (renderer); HMR in development; no separate webpack config | Plain webpack (no HMR, verbose config), Electron Forge with webpack plugin (heavier, less Vite-native) |

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
> - `drift-desktop` launches on Windows and macOS; drift-api sidecar starts with SQLite; drift-ui loads inside the Electron window
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
Then drift-cli compiles without errors

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
| A-1-T1 | PREPARATORY | Init Cargo workspace with `drift-cli`, `drift-api`, `drift-scanner` crates; `drift-dashboard` pnpm workspace | Sonnet | ≤ 2 000 |
| A-1-T2 | PREPARATORY | GitHub Actions CI: cargo test + clippy + pnpm lint + pnpm build | Sonnet | ≤ 1 500 |
| A-1-T3 | PREPARATORY | Docker Compose: postgres:16, drift-api, drift-dashboard for local dev | Sonnet | ≤ 1 500 |
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
drift check — API Contract Drift Monitor
════════════════════════════════════════
  BREAKING   user.phone         field_removed
  BREAKING   user.address.zip   type_changed  string→integer
  ok         user.nickname      field_added

2 breaking changes · 1 addition · exit 1
```

---

### Story A-5 · Policy File (`.drift.yml`)

> **Persona:** Platform engineer configuring CI behaviour per repo
> **Value:** So that teams can choose warn-only vs block without changing the CLI invocation
> **Priority:** P0
> **Size:** S
> **Dependencies:** A-4

**Acceptance Criteria**

```gherkin
Given .drift.yml sets block_on: never
When breaking changes are found
Then exit code is 0 (warn only)

Given .drift.yml sets block_on: active_consumers
When no consumers are registered
Then exit code is 0 (no active consumers known yet)

Given .drift.yml is malformed YAML
Then exit code is 2 with a clear error message
```

**Tasks**

| ID | Hat | Goal | Agent tier | Token budget |
|---|---|---|---|---|
| A-5-T1 | FEATURE | Parse `.drift.yml` config; default values when file absent | Sonnet | ≤ 1 500 |
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

**Security note:** `GITHUB_TOKEN` read from env only; never logged or included in drift-api payloads.

---

### Story A-8 · `drift-ui` + `drift-desktop` Electron Shell (SQLite mode)

> **Persona:** Internal engineer running the tool on their laptop for the first time
> **Value:** So that I can open a desktop app, point it at a spec, and see diffs without configuring any infrastructure
> **Priority:** P0 (tracer bullet requires end-to-end, including UI)
> **Size:** M
> **Dependencies:** A-1 (workspace skeleton)
> **DoR status:** READY after ADR-007, ADR-008, ADR-009 recorded

**Acceptance Criteria**

```gherkin
Given drift-desktop is launched on Windows or macOS
When it starts
Then drift-api sidecar is spawned automatically, pointing at a local SQLite file
And the drift-ui interface loads inside the Electron window

Given the user clicks "Run Check" and selects two spec files
When drift check completes
Then the Diff result appears in drift-ui without opening a terminal

Given the app is closed
Then the drift-api sidecar process is also terminated cleanly

Given drift-api is also accessible via HTTP on localhost during the session
Then drift-cli can connect to it for CI runs targeting the same local data
```

**Tasks**

| ID | Hat | Goal | Agent tier | Token budget |
|---|---|---|---|---|
| A-8-T1 | PREPARATORY | `drift-ui` pnpm workspace with Vite 6 + React 19 + TypeScript + Tailwind + shadcn/ui scaffold; `drift-desktop` pnpm workspace with electron-vite; shared `drift-ui` renderer | Sonnet | ≤ 2 500 |
| A-8-T2 | FEATURE | `drift-api` SQLite mode: `--db sqlite:PATH` flag; sqlx `AnyDatabase` feature; same migrations run on SQLite | Sonnet | ≤ 2 500 |
| A-8-T3 | FEATURE | Electron main process: spawn `drift-api` child process with SQLite path in `userData`; wait for health-check before opening window; terminate on app quit | Sonnet | ≤ 2 500 |
| A-8-T4 | FEATURE | Minimal `drift-ui` home screen: service list (empty state), "Run Check" button that calls drift-api via fetch; displays raw JSON result | Sonnet | ≤ 2 000 |
| A-8-T5 | FEATURE | electron-builder config: Windows NSIS installer, macOS DMG, Linux AppImage; GitHub Actions release job | Sonnet | ≤ 1 500 |

**Security note:** Electron `contextIsolation: true`, `nodeIntegration: false`. All Node.js access via `contextBridge` preload script. drift-api sidecar bound to `127.0.0.1` only — not exposed on the network.

**Hand-off artifact:** Updated Architecture Memory confirming IPC/HTTP boundary between drift-desktop and drift-api.

---

### Story A-7 · `drift-api` Stub — Diff Submission

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
| B-1-T3 | FEATURE | `drift register` CLI subcommand: reads `.drift.yml`, calls POST /v1/consumers + subscription | Sonnet | ≤ 2 000 |

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
> **Feature flag:** `DRIFT_RELEASE_NOTES_ENABLED=true` (Claude call gated)

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
| B-4-T1 | FEATURE | Fetch Diff + Blast Radius from drift-api; populate template structured sections deterministically | Sonnet | ≤ 2 500 |
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
> - drift-ui (full dashboard) shows cross-service trend view in both browser and Electron
> - Playground tab shows "Try It" for any registered producer's spec; sandbox environment pre-configured
> - PostgreSQL mode verified: same migrations, same API behaviour as SQLite mode
> - Web self-host confirmed: `docker compose up` brings up drift-api + PostgreSQL + drift-ui in browser

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
| C-9 | drift-ui full shell (sg-shell, sg-nav, dark theme, React Router) | M | Sonnet | A-8 |
| C-10 | drift-ui: Diffs list + Diff detail with blast radius table | M | Sonnet | C-9 |
| C-11 | drift-ui: KPI cards (breaking-changes-30d, consumers-at-risk) | S | Sonnet | C-10 |
| C-12 | Scalar Playground integration (service detail tab) — works in browser and Electron | M | Sonnet | C-9 |
| C-13 | Sandbox environment config (pre-sales base URL + auth injection) | S | Sonnet | C-12 |
| C-14 | PostgreSQL mode: drift-api `--db postgres://…` flag; Docker Compose for web self-host; migration parity test | M | Sonnet | A-8-T2 |
| C-15 | Web deployment: drift-api serves Vite static bundle from `/app`; nginx reverse proxy config | S | Sonnet | C-14 |
| C-16 | Design system token audit across drift-ui + Electron window chrome (§6 compliance) | S | Haiku | C-9–C-15 |

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
| 0.2 | 2026-05-17 | Yannick Verrydt | Electron + Web dual deployment: replaced Next.js with Vite 6; added drift-ui (shared renderer) and drift-desktop (Electron shell); added Story A-8 (Electron shell + SQLite mode); added ADR-007/008/009; added SQLite/PostgreSQL database abstraction (ADR-002 revised); expanded EPIC C with C-14/C-15/C-16; updated Architecture Memory, Glossary, and EPIC exit criteria |

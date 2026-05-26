# API Contract Radar — Development Maturity Plan

*Framework: AI-Native Software Delivery Core Specification v1.0 + ClaudeExtras Agent Suite*  
*Evaluator: current-state-evaluator-agent*  
*Date: 2026-05-26*

---

## CURRENT STATE EVALUATION

```
CURRENT STATE EVALUATION
=========================
Project:    API Contract Radar Monitor
Date:       2026-05-26
Evaluator:  current-state-evaluator-agent (C:\Projects\ClaudeExtras\core)

DIMENSION SCORES:
  1. Code Health:           6/10
  2. Architecture Health:   5/10
  3. Domain Model Health:   6/10
  4. Delivery Flow Health:  5/10
  5. Technical Debt Health: 6/10
  6. Operational Readiness: 6/10
  7. Product Value Health:  5/10

  OVERALL: 5.6/10

RECOMMENDED EXECUTION MODE:
  HARDENING → DELIVERY
  Rationale: Feature set is complete at v0.2.0 (EPICs A–L done). No major pivots
  pending. The 5-phase maturity plan transitions this back into DELIVERY mode for
  Phase 2+ work. Security and test hardening (Phases 4–5) operate in HARDENING mode.

CRITICAL FINDINGS (address immediately):
  - No fitness functions in CI (dependency rule, contract tests, performance baselines
    are all stated intent with zero automated verification)
  - No ADRs — key decisions (SQLite+Postgres dual target, Electron IPC bridge, SSRF
    guard strategy) exist as tribal knowledge only
  - SLOs absent — no latency targets defined for diff computation or blast-radius query;
    Prometheus endpoint exists but golden signals are not SLO-bound

TOP 5 IMPROVEMENTS (in impact order):
  1. SLOs for core operations → @agent-slo-advisor
     Impact: Quantifies "healthy" so degradation is detectable, not just felt
  2. ADRs for irreversible decisions → architectural-decision-recorder (ADR template)
     Impact: Onboarding, decision archaeology, prevents re-litigation
  3. Fitness functions in CI → @agent-evolutionary-architecture-advisor
     Impact: Prevents architectural regression silently accumulating between releases
  4. Security threat model → @agent-threat-model-facilitator + @agent-stride-threat-analyzer
     Impact: Surfaces attack surface before users find it
  5. Acceptance test coverage → @agent-acceptance-test-designer
     Impact: Playwright journeys catch regressions that unit tests cannot

STRENGTHS (protect these):
  - Ubiquitous language enforced in CLAUDE.md and throughout codebase — zero synonym drift
  - Test suite growing with each hardening sprint (255+ workspace tests, clippy -D warnings)
  - Runbook exists and is detailed — incident response is documented, not tribal
  - Structured logging (tracing crate) with request-ID correlation across all handlers
  - SSRF guard, OIDC authentication, org isolation — security baseline is real, not aspirational

NEXT EVALUATION: 2026-08-26 (quarterly)
```

---

## DIMENSION DETAIL

### 1. CODE HEALTH — 6/10

```
CODE HEALTH SCORE: 6/10
  Smell density:     LOW — < 5 per 1k lines in Rust; MODERATE in TSX (CsvRunnerPanel ~700 loc)
  Test coverage:     Backend ~70% estimated — trend: ↑ (hardening sprints added tests)
                     Frontend: 0% unit tests — no Jest/Vitest configured
  Test quality:      ADEQUATE (Rust) / ABSENT (frontend)
  TDD adherence:     PARTIAL — mandated in CLAUDE.md; not always verifiable from git history
  Duplication:       LOW-MODERATE — API client migration reduced 42 duplicated fetch patterns;
                     Rust query patterns repeated across modules (pagination, org_id binding)
  Readability:       CLEAR — naming follows domain glossary; handlers well-scoped

  Top issues:
    1. CsvRunnerPanel.tsx is ~700 lines (Long Module smell) → @agent-fowler-smell-detector,
       then split into subcomponents with @agent-refactoring-catalog-advisor
    2. Frontend test coverage absent — zero unit tests, no test runner configured
       → @agent-unit-test-coach (Vitest setup for React components)
    3. Repeated pagination/org_id pattern in Rust handlers (Data Clumps)
       → @agent-refactoring-catalog-advisor (extract PaginationQuery extractor)
```

### 2. ARCHITECTURE HEALTH — 5/10

```
ARCHITECTURE HEALTH SCORE: 5/10
  Dependency Rule:     MOSTLY CLEAN — radar-core holds pure domain types; radar-api
                       imports from radar-core correctly. CLI calls API via HTTP not
                       direct crate import. One violation: audit.rs imports chrono
                       directly (acceptable for infra module).
  Component cohesion:  MODERATE — radar-api is a monolith; all 22 modules in one
                       process with one DB connection pool. Appropriate for current
                       scale, but will constrain as feature set grows.
  Component coupling:  APPROPRIATE — crates are loosely coupled via radar-core types.
                       No circular dependencies detected.
  ADR coverage:        ABSENT — zero ADRs. Key decisions are tribal knowledge:
                       • SQLite+PostgreSQL dual target via sqlx::AnyPool
                       • Electron IPC bridge (window.drift) for base URL resolution
                       • SSRF guard implementation (DNS resolution vs regex)
                       • org_id = "default" hardcoded (multi-tenancy deferred)
  Fitness functions:   0 active / 5 missing:
                       • Dependency rule check (missing)
                       • Contract tests for radar-cli → radar-api (missing)
                       • Performance baseline for diff computation (missing)
                       • Security scan (missing)
                       • Bundle size budget for radar-ui (missing)
  Integration patterns: DELIBERATE — HTTP sync, webhook async with outbox pattern,
                        scheduled scans with tokio tasks. No circuit breakers on
                        outbound HTTP (webhook delivery, spec fetch).

  Top issues:
    1. Zero ADRs — tribal knowledge for all cross-cutting decisions
       → @agent-architectural-decision-recorder (write ADRs for top 5 decisions)
    2. No fitness functions in CI — architectural properties stated but not verified
       → @agent-evolutionary-architecture-advisor
    3. No circuit breakers on outbound HTTP calls (webhooks, scheduled scans)
       → @agent-stability-pattern-advisor
```

### 3. DOMAIN MODEL HEALTH — 6/10

```
DOMAIN MODEL HEALTH SCORE: 6/10
  Ubiquitous Language:   ENFORCED — CLAUDE.md has explicit table; zero synonym drift
                         found across 12 EPICs
  Domain layer purity:   PARTIAL — radar-core has pure types. radar-api mixes HTTP
                         handler logic with business rules (e.g., blast-radius computation
                         in diffs.rs handler rather than in radar-core).
  Anemic model:          PARTIAL — Rust structs are data containers; behaviour is in
                         free functions rather than methods on types. Acceptable in Rust
                         idioms but domain logic is scattered rather than cohesive.
  Aggregate boundaries:  UNCLEAR — no explicit aggregate design. Service → Diff → Consumer
                         relationship exists implicitly but invariants not enforced by types.
  Bounded Contexts:      IMPLICIT — radar-cli (CI context) and radar-api (web context)
                         share radar-core types. The boundary is physical (separate binaries)
                         but the relationship is Shared Kernel without documentation.
  Core Domain focus:     YES — drift detection + blast-radius computation receives the
                         most implementation attention (7 of 12 EPICs touch it directly)

  Top issues:
    1. Business logic in HTTP handlers (blast-radius query, policy decision) — should
       live in radar-core or domain service functions
       → @agent-layer-violation-detector, then @agent-refactoring-catalog-advisor
    2. org_id = "default" is a hardcoded assumption that will require shotgun surgery
       when multi-tenancy is enabled — implicit invariant, not enforced by types
       → @agent-aggregate-design-reviewer (design org_id as a value type)
    3. Bounded Context relationship between CLI and API is Shared Kernel (radar-core)
       without documentation — risk of accidental coupling
       → @agent-context-integration-advisor
```

### 4. DELIVERY FLOW HEALTH — 5/10

```
DELIVERY FLOW HEALTH SCORE: 5/10
  WIP:                 UNMANAGED — solo/AI-native development; no WIP limit policy.
                       Single-person teams are immune to WIP waste but vulnerable to
                       context-switching between phases.
  Cycle Time (85th):   UNKNOWN — no issue tracker or cycle time measurement in place.
  Throughput:          UNKNOWN — measured in EPICs/sprint informally.
  Flow Efficiency:     UNKNOWN — cannot assess without tracking.
  Flow Debt:           NONE — sequential EPIC delivery; no artificial aging observed.
  Backlog health:      DEEP — DEVELOPMENT_PLAN.md + 5-phase maturity plan serve as
                       ordered backlog. Stories are high-level (phase granularity) rather
                       than INVEST-compliant tasks.
  DoR enforced:        PARTIALLY — CLAUDE.md has TDD mandate and hat declarations;
                       no formal Definition of Ready per story.
  DoD strength:        ADEQUATE — cargo test + clippy -D warnings + pnpm lint.
                       Missing: coverage thresholds, security scan, performance baseline.
  Velocity misuse:     NONE — no velocity metric defined; work is self-paced.
  Unplanned work:      < 10% — bug fixes were folded into hardening sprints as planned.

  Top issues:
    1. DoD has no coverage threshold — tests can be deleted without CI failing
       → @agent-backlog-health-advisor (strengthen DoD)
    2. Backlog stories are phase-granular, not task-granular — hard to track progress
       within a phase or estimate completion
       → @agent-user-story-coach (break phases into INVEST stories)
    3. No cycle time measurement — cannot forecast delivery or detect flow degradation
       → @agent-flow-metrics-advisor (establish lightweight tracking)
```

### 5. TECHNICAL DEBT HEALTH — 6/10

```
TECHNICAL DEBT HEALTH SCORE: 6/10
  Debt visibility:       PARTIALLY — 5-phase maturity plan is an implicit debt register.
                         Items are named (ADRs missing, no fitness functions, no E2E tests)
                         but not tracked with principal/interest/servicing decision.
  Debt register items:   ~10 (see register below)
  Composition:
    Code debt:      2 items — CsvRunnerPanel size, no frontend tests
    Architecture:   4 items — no ADRs, no fitness functions, async op inconsistency,
                    SSRF guard duplicated
    Infra debt:     4 items — no automated release, no E2E suite, no readiness model,
                    IP-literal test URLs
  Recurring interest:    est. 0.5 days/sprint — occasional rework from missing ADRs;
                         manual testing gap in frontend
  Past tipping point:    0 items (debt is recent and manageable)
  Debt ratio:            ~10% of sprint capacity (low but growing as codebase matures)
  Credit Check:
    Business alignment:  GREEN (solo dev, no external schedule pressure)
    Development process: YELLOW (DoD has no coverage threshold; tests can silently shrink)
    Architecture:        YELLOW (no ADRs; decisions undocumented and hard to revisit)
    Team:                GREEN (experienced, domain-knowledgeable)

  Top issues:
    1. Debt register is implicit (in narrative docs) not explicit (tracked items)
       → @agent-technical-debt-strategist (formalise register with principal/interest)
    2. Architecture debt (no ADRs) compounds over time — each new contributor starts cold
       → Write top 5 ADRs as first Architecture Health action
    3. Infra debt: no automated release means Time to Market is manual and error-prone
       → @agent-deployment-pipeline-designer (automate release pipeline)
```

### 6. OPERATIONAL READINESS — 6/10

```
OPERATIONAL READINESS SCORE: 6/10
  Logging:              STRUCTURED — tracing crate with request-ID; span instrumentation
                        on all handlers. No log sampling or level configuration via env.
  Metrics:              BASIC — Prometheus /metrics endpoint; request_duration_seconds
                        histogram; rate_limit_rejections_total counter. Golden signals
                        (saturation, error rate per endpoint) not defined.
  Tracing:              NO — single-process service; no distributed trace propagation.
                        Not needed until multi-service architecture.
  SLOs:                 ABSENT — no SLI/SLO definitions. Known performance-sensitive
                        operations: diff computation, blast-radius query, CSV row execution.
  CI/CD:                PARTIAL — GitHub Actions: build + test + lint + Docker. No
                        automated deployment; no release pipeline.
  Deploy frequency:     MANUAL — git tag + manual build. No automated release.
  Rollback mechanism:   MANUAL — migration rollback documented in runbook; no feature
                        flags or canary deployment.
  Circuit breakers:     NO — reqwest client for webhooks/scans has no circuit breaker.
                        Downstream failure causes goroutine/task pile-up.
  Runbooks:             YES — docs/runbook.md comprehensive; covers CSV runner, retention,
                        two incident entries.

  Top issues:
    1. SLOs absent — cannot define "healthy" for monitoring without targets
       → @agent-slo-advisor (define SLIs for diff computation, blast radius, CSV execution)
    2. No circuit breakers on outbound HTTP — webhook delivery failure cascades
       → @agent-stability-pattern-advisor (circuit breaker for webhook outbox)
    3. Deployment is manual — slow Time to Market, inconsistent release process
       → @agent-deployment-pipeline-designer
```

### 7. PRODUCT VALUE HEALTH — 5/10

```
PRODUCT VALUE HEALTH SCORE: 5/10
  Vision:               CLEAR — "make API contract drift visible and blast-radius-aware
                        before it breaks consumers in production." Team can state this.
  Value measurement:    QUALITATIVE — no usage tracking, no analytics, no feature
                        adoption data. Features are assumed used, not measured.
  Feature usage rate:   UNKNOWN — no instrumentation. High risk: 64% of features in
                        typical products are used by < 20% of users.
  EBM - Current Value:  YELLOW — tool is production-ready but no real users yet to
                        generate CV signals.
  EBM - Time to Market: YELLOW — manual deployment; release cadence is EPIC-driven
                        (monthly), not value-driven (weekly).
  EBM - Ability to Innovate: GREEN — 80%+ of capacity goes to new features. Debt
                        interest is low (~10% of sprint capacity).
  Validation loop:      ABSENT — no user feedback mechanism, no Sprint Review with
                        external stakeholders, no analytics on feature usage.
  Mindset:              TRANSITIONING — was PROJECT (EPIC A-L delivery); now moving
                        toward PRODUCT (outcome-driven maturity phases).
  Top wastes (Poppendiecks):
    1. Extra features — no usage data means unknown % of 12 EPICs unused
    2. Partially done — E2E test suite started (framework selected) but not running
    3. Waiting — manual release process introduces unnecessary handoff delay

  Top issues:
    1. No validation loop — features are built on assumptions, not user evidence
       → @agent-product-owner-coach (instrument key flows, define success metrics)
    2. Feature usage unknown — 64% unused feature rate is industry baseline
       → Add usage events to audit_event table (already created in Phase 1)
    3. Mindset still PROJECT — EPICs as milestones rather than hypothesis-driven increments
       → @agent-lean-thinking-advisor (shift to value hypothesis per increment)
```

---

## DEVELOPMENT MATURITY PLAN

*Execution mode for Phases 1–3: DELIVERY. Phases 4–5: HARDENING.*

---

### Phase 1 — Runtime Foundation ✅ COMPLETE (v0.2.0, 2026-05-26)

**Agreed scope:**
- Centralize frontend API access into one client.
- Fix desktop/web API base URL handling (Electron `file://` breakage).
- Add shared error handling and auth behavior.
- Add audit events now, not later.
- Add setup/runtime diagnostics where they support debugging.

**Agents applied:** `@agent-refactoring-catalog-advisor`, `@agent-pragmatic-programmer`

**Outcomes (commit 654e50e):**
- `radar-ui/src/lib/apiClient.ts` — typed fetch wrapper; `initApiClient()` resolves Electron sidecar URL via IPC before first render; all 42+ fetch calls migrated.
- `audit_event` table (migration 030) + `GET/POST /v1/audit-events`.
- App.tsx `useAuth` hook uses api client; sidebar version bumped to v0.2.0.

---

### Phase 2 — Product Readiness Loop

**Execution mode:** DELIVERY
**Target:** v0.2.1

**Agreed scope:**
- Add `GET /v1/readiness`.
- Wire the dashboard around "what is monitored, what is missing, what changed, what needs action."
- Improve empty states and setup guidance.
- Make the core drift workflow feel intentional end to end.

**Agents to apply:**

| Agent | Task | Deliverable |
|---|---|---|
| `@agent-product-owner-coach` | Define what "configured" means; model first-run to first-diff flow | `GET /v1/readiness` checklist; dashboard readiness widget |
| `@agent-lean-thinking-advisor` | Identify wait steps and handoffs in setup flow | Empty states with actionable next steps; setup guidance |
| `@agent-observability-advisor` | Instrument key flows via audit_event | Usage events: `diff.created`, `consumer.registered`, `csv_run.started` |

**Stories:**
1. `GET /v1/readiness` — returns structured checklist: DB connected, migrations current, at least one service registered, at least one diff recorded, at least one consumer registered. Each item has `status` (ok/missing/warn) and a `hint` pointing to the UI page or CLI command that resolves it.
2. Dashboard wired around readiness — homepage replaces generic summary cards with a readiness model: what is monitored (service count, consumer count), what is missing (items from readiness checklist with hints), what changed recently (last 5 diffs with severity), what needs action (unacknowledged breaking changes, policy blocks).
3. Instrument three core flows with `audit_event` records — diff created (in `diffs.rs`), consumer registered (in `consumers.rs`), CSV run started (in `csv_runner.rs`).
4. Empty state improvements — Services, Diffs, Consumers, and Catalog Sources pages: replace generic empty messages with contextual cards stating what the page does, what the prerequisite is, and the command or button to take the next step.

**DoD additions:**
- `GET /v1/readiness` has an integration test covering each checklist item state
- Dashboard readiness widget renders in all empty-state permutations (verified manually)

---

### Phase 3 — Async Operations Hardening

**Execution mode:** DELIVERY
**Target:** v0.3.0

**Agreed scope:**
- Do not build a generic job platform.
- CSV Runner is the reference implementation for how long-running user actions should behave. All six maturity findings (error counting, row persistence, history UI, cancellation granularity, retry safety, status clarity) are resolved as of v0.2.0.
- Apply the same pattern individually to: scheduled scans, webhooks, and release notes generation.
- Each should have consistent: status semantics, history, cancellation where relevant, retry behavior, audit records, retention, and useful failure reporting.

**Reference implementation — CSV Runner (complete as of v0.2.0):**
- Status: `pending` → `running` → `completed` / `completed_with_failures` / `failed` / `cancelled`
- History: paginated list of past runs accessible in the UI
- Cancellation: checked at row granularity, mid-retry
- Retry: safe methods always retry; unsafe only with explicit `enable_retry: true`
- Audit: job start/end recorded in `audit_event`
- Retention: purged by 1-hour background job after configurable window
- Failure reporting: per-row `error` + `error_count` on job; amber status badge for partial failures

**Agents to apply:**

| Agent | Task | Deliverable |
|---|---|---|
| `@agent-stability-pattern-advisor` | Consistent retry + failure reporting for webhook outbox and scan executor | Webhook: `completed_with_failures`; scan: `failed` with stored error |
| `@agent-unit-test-coach` | Replace IP-literal test URLs with in-process echo server | Axum server spawned in `#[tokio::test]`; no DNS in CI |
| `@agent-test-architecture-advisor` | Test recipe for async state transitions | `wait_for_status(pool, job_id, "completed")` helper with timeout |

**Stories:**
1. Webhooks — add per-delivery `status` (`queued`/`delivered`/`failed`), `completed_with_failures` on the webhook record when any delivery fails, failure reason visible in SettingsPage delivery history. Add audit records for delivery attempts.
2. Scheduled scans — add `last_run_status` + `last_run_error` + `last_run_at` columns; surface in the scheduled scans list UI; retain run history entries under the same retention window as CSV runs. Add audit records for run start/end.
3. Release notes generation — replace fire-and-forget pattern with async job row; add `GET /v1/release-notes/:id/generate-status`; DiffDetailPage polls until complete or shows inline error. Add audit record for generation start/complete.
4. In-process mock HTTP server — shared test helper that spawns a local Axum server returning configurable status codes; used by CSV Runner, scan, and webhook tests. Replaces the `93.184.216.34` IP literal workaround throughout.

---

### Phase 4 — Security Hardening

**Execution mode:** HARDENING
**Target:** v0.3.1

**Agreed scope:**
- Add simple host allowlist / network policy first.
- Normalize SSRF checks into reusable platform code (currently duplicated between CSV runner and scan executor).
- Keep "never return raw secrets" as the immediate guarantee.
- Scope encryption at rest as deployment/enterprise work unless SQLCipher or Postgres column encryption becomes a deliberate architectural choice (record the decision either way).

**Agents to apply:**

| Agent | Task | Deliverable |
|---|---|---|
| `@agent-threat-model-facilitator` | DFD for radar-api; trust boundary identification | Data Flow Diagram; trust boundary list |
| `@agent-stride-threat-analyzer` | STRIDE per element: API inputs, webhook delivery, spec fetch, CSV runner, OIDC callback | Threat table (element × STRIDE × severity) |
| `@agent-secure-design-reviewer` | For each HIGH/CRITICAL threat: choose control; verify test exists | Mitigation table; test stubs for gaps |
| `@agent-privacy-threat-modeler` | PII in `audit_event.meta`; org isolation review; bearer token in logs | Privacy classification; redaction rules |

**Stories:**
1. Host allowlist — `RADAR_ALLOWED_HOSTS` env var (comma-separated glob patterns, e.g. `*.internal,api.github.com`); when set, outbound HTTP in CSV runner and scan executor is blocked unless the resolved hostname matches. Default: empty (no restriction beyond SSRF guard).
2. Normalize SSRF guard — extract `is_ssrf_blocked(url)` from `csv_runner.rs` into `radar_api::utils::ssrf`; both CSV runner and scan executor call the same function; tests cover all bypass vectors (redirect policy already set to `none()`).
3. Secret masking — bearer tokens and API keys in `audit_event.meta` redacted to `[REDACTED]` before insert; sandbox env `bearer_token` field never returned in `GET /v1/sandbox-envs` response (verify GET path in addition to the existing PUT guard).
4. Encryption at rest — record the architectural decision as ADR-003: "SQLite deployments rely on OS filesystem encryption (FileVault/BitLocker); Postgres deployments rely on infrastructure-level storage encryption. No SQLCipher dependency introduced. Column-level encryption deferred until a specific compliance requirement names it." No code change.
5. STRIDE test coverage — one test per HIGH threat asserting the mitigation holds (injection, auth bypass, SSRF redirect, information disclosure via error body).

**DoD additions (HARDENING mode):**
- `cargo audit` returns no HIGH or CRITICAL CVEs
- All HIGH threats have a passing test for the mitigation
- Secrets never appear in `audit_event` rows or API response bodies (grep assertion in CI)

---

### Phase 5 — Test and Release Maturity

**Execution mode:** HARDENING → DELIVERY
**Target:** v0.4.0

**Agreed scope:**
- Add focused Playwright journeys.
- For CSV Runner, explicitly include a spawned local echo/test API server in CI.
- Add backend integration tests around org scoping, security boundaries, and async state transitions.
- Add smoke coverage for packaged desktop mode, not just Vite dev mode.

**Agents to apply:**

| Agent | Task | Deliverable |
|---|---|---|
| `@agent-acceptance-test-designer` | Playwright journeys in domain language | 5 critical golden paths |
| `@agent-deployment-pipeline-designer` | Automated release pipeline | GitHub Actions: tag → artifacts → GitHub Release |
| `@agent-evolutionary-architecture-advisor` | Fitness functions in CI | `cargo deny`, dependency rule check, bundle size budget |
| `@agent-architectural-decision-recorder` | Top 5 ADRs documented | `docs/adr/001` through `docs/adr/005` |

**Stories:**
1. Playwright E2E journeys — 5 critical golden paths: (a) first diff: register service → upload spec → compare → view diff detail; (b) consumer registration → subscription → blast radius shows consumer; (c) CSV run: upload CSV → configure template → run → inspect results → export failed rows; (d) webhook registration → test fire → verify delivery in history; (e) playground compare: paste two specs → diff inline. Each journey runs against a live `radar-api` with a test SQLite DB.
2. CSV Runner CI with echo server — `cargo test -p radar-api` starts an in-process Axum echo server on a random port (built in Story 4 of Phase 3); CSV run tests use `http://127.0.0.1:{port}/echo`. Verifies retry logic, cancellation, body capture, and `completed_with_failures` status without network access.
3. Backend integration tests for org scoping, security boundaries, and async state transitions — asserts: (a) data from org A is not returned to org B requests; (b) unauthenticated requests to auth-required endpoints return 401, not 500; (c) async job state machine is enforced (`running` cannot transition to `pending`; `completed` cannot transition to `running`).
4. Packaged desktop smoke tests — `pnpm --filter radar-desktop dist` produces the installer; a smoke test script launches the packaged Electron binary, waits for the sidecar health check at `http://127.0.0.1:17380/health`, and asserts 200. Runs as a separate CI job on tagged releases only.
5. Automated release pipeline — GitHub Actions: on push of `v*` tag → `cargo build --release -p radar-api` → `pnpm build:ui` → `electron-builder` → upload `.exe`/`.dmg` as release artifacts → create GitHub Release with the matching CHANGELOG section prepended.

---

## AGENT SELECTION SUMMARY

| Phase | Primary Agents | Supporting Agents |
|---|---|---|
| 1 (done) | `refactoring-catalog-advisor`, `pragmatic-programmer` | — |
| 2 | `product-owner-coach`, `lean-thinking-advisor` | `observability-advisor` |
| 3 | `stability-pattern-advisor`, `unit-test-coach` | `test-architecture-advisor` |
| 4 | `threat-model-facilitator`, `stride-threat-analyzer`, `secure-design-reviewer` | `privacy-threat-modeler` |
| 5 | `acceptance-test-designer`, `deployment-pipeline-designer` | `evolutionary-architecture-advisor`, `architectural-decision-recorder` |

**All agents sourced from:** `C:\Projects\ClaudeExtras\` — DDD Full Suite, DevOps Suite, Security Engineering Suite, SRE & Observability Suite, Testing Strategy Suite.

---

## TECHNICAL DEBT REGISTER

| ID | Artifact | Type | Cause | Principal | Interest | Decision |
|---|---|---|---|---|---|---|
| TD-01 | No ADRs | Architecture | Speed during EPIC delivery | 2d | 0.5d/sprint | Pay in Phase 5 |
| TD-02 | No fitness functions | Architecture | CI not extended past lint/test | 1d | 0.2d/sprint | Pay in Phase 5 |
| TD-03 | Webhook/scan/release-note async inconsistency | Architecture | CSV Runner was first; others deferred | 2d | 0.5d/incident | Pay in Phase 3 |
| TD-04 | SSRF guard duplicated in csv_runner + scans | Code | Organic growth | 0.5d | 0.1d/sprint | Pay in Phase 4 |
| TD-05 | Frontend 0% unit tests | Code | No test runner configured | 2d | 0.3d/sprint | Pay in Phase 5 |
| TD-06 | No readiness model or intentional empty states | Infra | Not a focus during EPIC delivery | 1d | Onboarding friction | Pay in Phase 2 |
| TD-07 | Manual release | Infra | No release automation | 1d | 0.5d/release | Pay in Phase 5 |
| TD-08 | No threat model | Infra | Security deferred intentionally | 2d | Risk (unquantified) | Pay in Phase 4 |
| TD-09 | org_id = "default" | Architecture | Multi-tenancy deferred intentionally | 5d | Shotgun surgery when enabled | Accept for now |
| TD-10 | IP-literal test URLs (DNS-dependent) | Infra | Offline CI workaround | 0.5d | Flaky in some CI environments | Pay in Phase 3 |

**Total estimated principal:** ~17 engineering days
**Recurring interest:** ~2.1 days/sprint
**Debt ratio:** ~10% (acceptable; below 15% warning threshold)

---

## NEXT EVALUATION

**Date:** 2026-08-26 (quarterly)
**Trigger conditions for early evaluation:**
- Score in any dimension drops by 2+ points
- New team member joins (onboarding diagnostic)
- Major architectural change proposed (multi-tenancy, new service)
- Incident with MTTR > 4h (operational readiness diagnostic)

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
  Debt register items:   ~12 (extracted from maturity plan and hardening sprint history)
  Composition:
    Code debt:      3 items — CsvRunnerPanel size, repeated query patterns, no frontend tests
    Architecture:   5 items — no ADRs, no fitness functions, no circuit breakers,
                    org_id hardcoded, no contract tests between CLI and API
    Infra debt:     4 items — no automated release, no SLOs, no E2E test suite, no
                    bundle size budget
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

**Rationale (from evaluation):** Architecture debt + code duplication from 42+ scattered fetch calls.

**Agents applied:**
- `@agent-refactoring-catalog-advisor` — Extract Move pattern for apiClient.ts
- `@agent-pragmatic-programmer` — DRY principle; single base URL resolution

**Outcomes:**
- `radar-ui/src/lib/apiClient.ts` — centralized typed fetch wrapper
- `initApiClient()` resolves Electron sidecar URL before first render
- All 42+ fetch calls migrated; Electron production file:// breakage fixed
- `audit_event` table (migration 030) + `GET/POST /v1/audit-events`
- App.tsx sidebar version bumped to v0.2.0

---

### Phase 2 — Product Readiness Loop

**Execution mode:** DELIVERY  
**Target:** v0.2.1  
**Rationale (from evaluation):** Product Value Health 5/10; SLOs absent; validation loop absent; empty states are generic.

**Agents to apply:**

| Agent | Task | Deliverable |
|---|---|---|
| `@agent-slo-advisor` | Define SLIs + SLO targets for diff computation, blast-radius query, CSV row execution | SLO doc + 3 Prometheus recording rules |
| `@agent-observability-advisor` | Golden signals per endpoint; error rate alert | 2 new Prometheus counters; alerting rule template |
| `@agent-product-owner-coach` | Readiness model: what does "configured" mean? | `GET /v1/readiness` endpoint; dashboard wired around it |
| `@agent-lean-thinking-advisor` | Map first-run to first-diff flow; identify wait steps | Improved empty states; setup guidance in UI |

**Stories:**
1. `GET /v1/readiness` — returns checklist: db connected, migrations current, at least one service registered, at least one diff recorded. Dashboard shows readiness widget.
2. Instrument core flows with usage events into `audit_event` (diff created, consumer registered, csv run started) — closes validation loop.
3. Define 3 SLOs: diff compute < 5s (p95), blast-radius query < 500ms (p95), CSV row < 10s (p95). Add recording rules to Prometheus scrape.
4. Improve empty states on Services, Diffs, Consumers pages — replace generic "nothing here" with actionable setup guidance tied to the readiness model.

**DoD additions for this phase:**
- `GET /v1/readiness` has integration test
- SLO targets documented in `docs/slos.md`
- Prometheus recording rules validated against test data

---

### Phase 3 — Async Operations Hardening

**Execution mode:** DELIVERY  
**Target:** v0.3.0  
**Rationale (from evaluation):** Architecture Health 5/10; no circuit breakers; webhook + scheduled scan async paths lack same rigor as CSV Runner.

**CSV Runner is the reference implementation.** It has: per-row retry (safe-method-aware), cancellation at row granularity, `completed_with_failures` status, retention, history UI, export. Apply this pattern to:

| Agent | Task | Deliverable |
|---|---|---|
| `@agent-stability-pattern-advisor` | Circuit breaker for webhook outbox; bulkhead for scan executor | `reqwest::redirect::Policy::none()` already done; add circuit-breaker state machine to webhook dispatcher |
| `@agent-unit-test-coach` | Spawned echo server for CSV Runner tests in CI; async path coverage | Replace `93.184.216.34` test literals with in-process mock server |
| `@agent-test-architecture-advisor` | Test pyramid: unit + integration + E2E layer design | Playwright config; test recipe for async job assertion |
| `@agent-reliability-review-facilitator` | PRR checklist for CSV Runner, webhooks, scheduled scans | Production readiness review doc; runbook entries |

**Stories:**
1. Webhook delivery — add `completed_with_failures` status, retry visibility in SettingsPage, delivery history pagination.
2. Scheduled scan history — add status tracking (running/completed/failed), same status semantics as CSV Runner.
3. Release notes generation — same async job pattern (currently fire-and-forget with polling; add job row, status endpoint, cancellation).
4. Circuit breaker for outbound HTTP — simple state machine (closed → open after N failures → half-open after timeout) in `webhooks.rs` and `scans.rs`.
5. In-process mock HTTP server for async tests (replaces DNS-dependent IP literals).

---

### Phase 4 — Security Hardening

**Execution mode:** HARDENING  
**Target:** v0.3.1  
**Rationale (from evaluation):** No formal threat model; security decisions are ad-hoc; SSRF guard is implemented but not tested against all bypass vectors.

**Agents to apply:**

| Agent | Task | Deliverable |
|---|---|---|
| `@agent-threat-model-facilitator` | DFD for radar-api; trust boundary identification | Data Flow Diagram; trust boundary list |
| `@agent-stride-threat-analyzer` | STRIDE per element: API inputs, webhook delivery, spec fetch, CSV runner, OIDC callback | Threat enumeration table (element × STRIDE category) |
| `@agent-secure-design-reviewer` | For each threat: choose strategy (fix/accept/avoid/transfer); select controls | Mitigation table with control descriptions |
| `@agent-owasp-security-tester` | Map OWASP Top 10 to existing tests; identify gaps | Gap list; 5 new security test stubs |
| `@agent-privacy-threat-modeler` | PII in audit_event (actor field, meta JSON); org isolation review | Privacy classification; retention policy recommendation |

**Stories:**
1. Host allowlist / network policy — configurable `RADAR_ALLOWED_HOSTS` environment variable; SSRF guard normalized to single implementation used by CSV runner, scan executor, and any future outbound HTTP.
2. Secret masking hardening — bearer tokens in `audit_event.meta` must be redacted before insert; API keys in sandbox envs stored with `Some(t) if !t.is_empty()` pattern extended to headers.
3. Input validation — request body size limits per endpoint type (diff specs: existing 4MB; CSV: unlimited bytes today → bound to row limit); OpenAPI schema validation middleware.
4. ADRs for security decisions — document SSRF guard choice, auth strategy, org isolation approach (5 ADRs in `docs/adr/`).
5. STRIDE test coverage — at least one test per high/critical threat that verifies the mitigation.

**DoD additions for HARDENING phase:**
- `cargo audit` clean (no known CVEs in dependencies)
- All HIGH threats have a tested mitigation
- Privacy classification documented for all PII-adjacent fields

---

### Phase 5 — Test and Release Maturity

**Execution mode:** HARDENING → DELIVERY  
**Target:** v0.4.0  
**Rationale (from evaluation):** Frontend test coverage 0%; no automated release; DORA metrics unknown.

**Agents to apply:**

| Agent | Task | Deliverable |
|---|---|---|
| `@agent-acceptance-test-designer` | Playwright E2E journeys in domain language | 5 critical journeys: first diff, consumer registration, CSV run, webhook delivery, playground compare |
| `@agent-deployment-pipeline-designer` | Automated release pipeline | GitHub Actions: tag → build → sign → publish release artifacts |
| `@agent-dora-metrics-advisor` | Establish DORA baseline | Deployment frequency, lead time, MTTR, CFR measurements |
| `@agent-evolutionary-architecture-advisor` | Fitness functions in CI | Dependency rule check; bundle size budget; API contract test between CLI and API |
| `@agent-architectural-decision-recorder` | Top 5 ADRs documented | `docs/adr/001` through `docs/adr/005` |

**Stories:**
1. Playwright E2E suite — 5 golden-path journeys; run on every PR; screenshots on failure.
2. Vitest + React Testing Library — configure for `radar-ui`; unit tests for `apiClient.ts`, `csvExporter.ts`, `variableResolver.ts`.
3. Automated release pipeline — GitHub Actions: on `v*` tag push → cargo build release → pnpm build:ui → electron-builder → upload artifacts → create GitHub Release with CHANGELOG entry.
4. DORA dashboard — weekly GitHub Action that measures deployment frequency and lead time from git history; outputs to a `metrics/dora.json` file.
5. Fitness functions — (a) `cargo deny` for license + CVE; (b) `import-boundaries` check (no radar-api imports in radar-cli except via HTTP); (c) bundle size < 2MB gzip for radar-ui.

---

## AGENT SELECTION SUMMARY

| Phase | Primary Agents | Supporting Agents |
|---|---|---|
| 1 (done) | `refactoring-catalog-advisor`, `pragmatic-programmer` | — |
| 2 | `slo-advisor`, `observability-advisor`, `product-owner-coach` | `lean-thinking-advisor` |
| 3 | `stability-pattern-advisor`, `unit-test-coach`, `test-architecture-advisor` | `reliability-review-facilitator` |
| 4 | `threat-model-facilitator`, `stride-threat-analyzer`, `secure-design-reviewer` | `owasp-security-tester`, `privacy-threat-modeler` |
| 5 | `acceptance-test-designer`, `deployment-pipeline-designer`, `dora-metrics-advisor` | `evolutionary-architecture-advisor`, `architectural-decision-recorder` |

**All agents sourced from:** `C:\Projects\ClaudeExtras\` — see DDD Full Suite (craft agents), DevOps Suite, Security Engineering Suite, SRE & Observability Suite, Testing Strategy Suite.

---

## TECHNICAL DEBT REGISTER

| ID | Artifact | Type | Cause | Principal | Interest | Decision |
|---|---|---|---|---|---|---|
| TD-01 | No ADRs | Architecture | Speed during EPIC delivery | 2d (onboarding cost) | 0.5d/sprint (re-litigation) | Pay in Phase 5 |
| TD-02 | No fitness functions | Architecture | CI not extended past lint/test | 1d | 0.2d/sprint (silent drift) | Pay in Phase 5 |
| TD-03 | No circuit breakers | Architecture | Not needed for EPIC delivery | 1.5d | 0.5d/incident | Pay in Phase 3 |
| TD-04 | CsvRunnerPanel 700 loc | Code | Feature growth without split | 0.5d | 0.1d/sprint (slow edits) | Pay in Phase 3 |
| TD-05 | Frontend 0% unit tests | Code | No test runner configured | 2d | 0.3d/sprint (manual verify) | Pay in Phase 5 |
| TD-06 | No SLOs | Infra | Not a focus during EPIC delivery | 0.5d | Unknown (incidents unmeasured) | Pay in Phase 2 |
| TD-07 | Manual release | Infra | No release automation built | 1d | 0.5d/release | Pay in Phase 5 |
| TD-08 | No STRIDE threat model | Infra | Security deferred to Phase 4 | 2d | Risk (unquantified) | Pay in Phase 4 |
| TD-09 | org_id = "default" | Architecture | Multi-tenancy deferred intentionally | 5d | Shotgun surgery when enabled | Accept for now |
| TD-10 | No usage analytics | Infra | Validation loop absent | 1d | Feature waste (unknown %) | Pay in Phase 2 |

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

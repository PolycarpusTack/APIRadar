# API Contract Radar — Development Maturity Plan

*Assessment framework: current-state-evaluator-agent (C:\Projects\ClaudeExtras\core)*
*Date: 2026-05-26 | Evaluation cycle: quarterly | Next: 2026-08-26*

---

## MATURITY TARGET

API Contract Radar is mature when a user can reliably:

1. **Register or import APIs** — discover services, producers, and consumers without manual data entry.
2. **Detect meaningful contract drift** — compare specs and receive a clear verdict on what changed and how severe.
3. **See affected consumers and blast radius** — understand which consumers are impacted and by how much, with evidence.
4. **Apply policy or approval rules** — block, warn, or override based on organization policy without CLI intervention.
5. **Receive reliable notifications** — webhooks, Slack, scheduled scans, and email digests that deliver consistently.
6. **Audit what happened afterward** — every policy decision, acknowledgement, and async operation is recorded and queryable.

Every improvement in this plan serves one or more of these six outcomes. If a proposed improvement does not serve them, defer it.

---

## CURRENT STATE EVALUATION

```
CURRENT STATE EVALUATION
=========================
Project:    API Contract Radar Monitor
Date:       2026-05-26

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
  Rationale: Feature set is complete at v0.2.0 (EPICs A–L done). The 5-phase
  maturity plan transitions back into DELIVERY mode for Phases 2–3. Security
  and test hardening (Phases 4–5) operate in HARDENING mode.

CRITICAL FINDINGS (address immediately):
  - No fitness functions in CI — dependency rule, contract tests, and
    performance baselines are all stated intent with zero automated verification.
  - No ADRs — key decisions (SQLite+Postgres dual target, Electron IPC bridge,
    SSRF guard strategy) exist as tribal knowledge only.
  - Audit events wired in Phase 1 but not yet used to instrument core flows —
    the validation loop is installed but not recording.
  - Packaged desktop mode untested — CI only covers Vite dev mode.
    Production Electron regressions go undetected.

TOP 5 IMPROVEMENTS (in product-impact order):
  1. Wire the dashboard around the product loop: what is monitored, what is
     missing, what changed, what needs action.
     Support: @agent-product-owner-coach, @agent-lean-thinking-advisor
  2. Instrument core flows (diff.created, consumer.registered, csv_run.started)
     into audit_event to close the product validation loop.
     Support: @agent-observability-advisor
  3. Define SLOs for diff computation, blast-radius queries, and CSV execution.
     Support: @agent-slo-advisor
  4. Apply consistent async operation behavior (status, history, cancellation,
     retry, failure reporting) to webhooks, scheduled scans, and release-note
     generation — matching the CSV Runner reference implementation.
     Support: @agent-stability-pattern-advisor
  5. Add focused Playwright journeys covering the six maturity-target outcomes.
     Support: @agent-acceptance-test-designer

STRENGTHS (protect these):
  - Ubiquitous language enforced in CLAUDE.md and throughout codebase — zero synonym drift.
  - Test suite growing with each hardening sprint (255+ workspace tests, clippy -D warnings).
  - Runbook exists and is detailed — incident response is documented, not tribal.
  - Structured logging (tracing crate) with request-ID correlation across all handlers.
  - SSRF guard, OIDC authentication, org isolation — security baseline is real, not aspirational.
  - CSV Runner is a complete reference implementation for async operations.

NEXT EVALUATION: 2026-08-26 (quarterly)
```

---

## DIMENSION DETAIL

### 1. CODE HEALTH — 6/10

```
CODE HEALTH SCORE: 6/10
  Smell density:     LOW in Rust (estimated < 5 per 1k lines); MODERATE in TSX
                     (CsvRunnerPanel ~700 loc, Long Module smell)
  Test coverage:     Backend: ~70% (estimate, unmeasured); trend: ↑
                     Frontend: 0% — no Vitest configured
  Test quality:      ADEQUATE (Rust) / ABSENT (frontend)
  TDD adherence:     PARTIAL — mandated in CLAUDE.md; not verifiable from git history
  Duplication:       LOW-MODERATE — apiClient.ts migration eliminated 42 duplicated
                     fetch patterns; Rust pagination/org_id pattern repeated across modules
  Readability:       CLEAR — naming follows domain glossary; handlers well-scoped

  Top findings:
    1. CsvRunnerPanel.tsx is ~700 lines — candidate for component decomposition.
    2. Frontend unit tests absent — no test runner configured.
    3. Repeated pagination/org_id query pattern in Rust handlers (Data Clumps smell).
```

### 2. ARCHITECTURE HEALTH — 5/10

```
ARCHITECTURE HEALTH SCORE: 5/10
  Dependency Rule:     MOSTLY CLEAN — radar-core holds pure domain types; radar-api
                       imports from radar-core correctly; CLI calls API via HTTP.
  Component cohesion:  MODERATE — radar-api is a monolith (22 modules, one process,
                       one DB pool). Appropriate for current scale.
  ADR coverage:        ABSENT — zero ADRs. Decisions held as tribal knowledge:
                       • SQLite+PostgreSQL dual target via sqlx::AnyPool
                       • Electron IPC bridge (window.drift) for base URL resolution
                       • SSRF guard implementation (DNS resolution, not regex)
                       • org_id = "default" (multi-tenancy deferred)
  Fitness functions:   0 active / 5 missing:
                       • Dependency rule enforcement in CI
                       • Contract tests radar-cli → radar-api
                       • Performance baseline for diff computation
                       • Security scan (cargo audit)
                       • Bundle size budget for radar-ui
  Integration patterns: DELIBERATE — HTTP sync, webhook async with outbox, scheduled
                        scans via tokio tasks. No circuit breakers on outbound HTTP.

  Top findings:
    1. Zero ADRs — every cross-cutting decision is undocumented.
    2. No fitness functions — architectural properties degrade silently between releases.
    3. No circuit breakers on outbound HTTP — webhook/scan failure is not bounded.
```

### 3. DOMAIN MODEL HEALTH — 6/10

```
DOMAIN MODEL HEALTH SCORE: 6/10
  Ubiquitous Language:   ENFORCED — CLAUDE.md table; zero synonym drift across 12 EPICs.
  Domain layer purity:   PARTIAL — radar-core has pure types; radar-api mixes HTTP
                         handler logic with business rules (blast-radius in diffs.rs).
  Anemic model:          PARTIAL — Rust structs as data containers is idiomatic, but
                         domain logic is in free functions rather than cohesive services.
  Aggregate boundaries:  UNCLEAR — Service → Diff → Consumer relationship is implicit.
  Bounded Contexts:      IMPLICIT — CLI (CI context) and API (web context) share
                         radar-core types as Shared Kernel without documentation.
  Core Domain focus:     YES — drift detection + blast-radius computation receives
                         most implementation attention (7 of 12 EPICs).

  Top findings:
    1. Business rules in HTTP handlers rather than domain service functions.
    2. org_id = "default" will require shotgun surgery when multi-tenancy is enabled.
    3. Shared Kernel relationship between CLI and API is undocumented — risk of
       accidental coupling.
```

### 4. DELIVERY FLOW HEALTH — 5/10

```
DELIVERY FLOW HEALTH SCORE: 5/10
  WIP:                 UNTRACKED — solo/AI-native development has no formal WIP
                       policy. Solo development is still vulnerable to WIP waste
                       via context-switching between phases and deferred tasks.
  Cycle Time (85th):   UNKNOWN — no issue tracker or measurement.
  Throughput:          UNKNOWN — measured informally in EPICs/month.
  Flow Efficiency:     UNKNOWN — cannot assess without tracking.
  Flow Debt:           LOW — sequential EPIC delivery; no artificial aging observed.
  Backlog health:      ADEQUATE — maturity plan serves as ordered backlog, but
                       stories are phase-granular rather than INVEST-compliant tasks.
  DoD strength:        ADEQUATE — cargo test + clippy -D warnings + pnpm lint.
                       Missing: coverage threshold, security scan, performance gate.
  Unplanned work:      < 10% (estimate) — bug fixes folded into hardening sprints.

  Top findings:
    1. DoD has no coverage threshold — tests can shrink without CI detecting it.
    2. Backlog is phase-granular — hard to track intra-phase progress or estimate.
    3. No cycle time tracking — cannot detect or forecast flow degradation.
```

### 5. TECHNICAL DEBT HEALTH — 6/10

```
TECHNICAL DEBT HEALTH SCORE: 6/10
  Debt visibility:       PARTIALLY — maturity plan is an implicit debt register.
                         Items are named but not tracked with principal/interest.
  Debt register items:   10 (see register at end of document)
  Composition:
    Code debt:      2 items — CsvRunnerPanel size, no frontend tests
    Architecture:   4 items — no ADRs, no fitness functions, async inconsistency,
                    SSRF guard duplicated
    Infra debt:     4 items — no automated release, no E2E suite, no readiness
                    model, IP-literal test URLs
  Recurring interest:    ~0.5 days/sprint (estimate) — rework from undocumented
                         decisions; manual frontend verification
  Past tipping point:    0 items
  Debt ratio:            ~10% of sprint capacity (estimate; not measured)
  Credit Check:
    Business alignment:  GREEN (self-paced; no external schedule pressure)
    Development process: YELLOW (DoD lacks coverage threshold)
    Architecture:        YELLOW (no ADRs; decisions undocumented)
    Team:                GREEN (experienced, domain-knowledgeable)

  Top findings:
    1. Debt register is narrative-only — no principal/interest/servicing tracking.
    2. Architecture debt compounds — each new contributor starts cold without ADRs.
    3. Manual release means Time to Market is inconsistent and slow.
```

### 6. OPERATIONAL READINESS — 6/10

```
OPERATIONAL READINESS SCORE: 6/10
  Logging:              STRUCTURED — tracing crate with request-ID; span on all handlers.
  Metrics:              BASIC — Prometheus /metrics; request_duration_seconds histogram;
                        rate_limit_rejections_total counter. Error rates not tracked.
  Tracing:              NOT APPLICABLE — single-process service.
  SLOs:                 ABSENT — no SLI/SLO definitions for any operation.
  CI/CD:                PARTIAL — GitHub Actions: build + test + lint + Docker image.
                        No automated deployment or release pipeline.
  Deploy frequency:     MANUAL — git tag + manual build.
  Rollback:             MANUAL — migration rollback documented in runbook.
  Circuit breakers:     ABSENT — reqwest for webhooks/scans; downstream failure
                        can pile up tasks.
  Runbooks:             YES — docs/runbook.md; covers CSV runner, retention,
                        two incident playbooks.

  Top findings:
    1. SLOs absent — no definition of "healthy" to detect degradation early.
    2. No circuit breakers — outbound HTTP failure is unbounded.
    3. Packaged desktop mode is untested — CI only covers Vite dev mode.
```

### 7. PRODUCT VALUE HEALTH — 5/10

```
PRODUCT VALUE HEALTH SCORE: 5/10
  Vision:               CLEAR — "make API contract drift visible and blast-radius-aware
                        before it breaks consumers in production."
  Value measurement:    NONE — no usage tracking or analytics. Features are assumed
                        used, not measured.
  Feature usage rate:   UNKNOWN — no instrumentation. Note: industry data suggests ~64%
                        of features are used by < 20% of users; treat this as a
                        motivating reference, not a measured fact for this project.
  EBM - Current Value:  YELLOW — production-ready but no real-user signals yet.
  EBM - Time to Market: YELLOW — manual release process; EPIC-driven cadence.
  EBM - Ability to Innovate: GREEN — ~80%+ capacity on new features (estimate);
                        debt interest is low.
  Validation loop:      ABSENT — audit_event table installed (Phase 1) but not yet
                        recording core product flows.
  Mindset:              TRANSITIONING — from PROJECT (EPIC delivery) to PRODUCT
                        (outcome-driven maturity).
  Top wastes:
    1. Partially done — E2E test suite not yet running.
    2. Waiting — manual release process delays value delivery.
    3. Unknown — no usage data means waste cannot be measured.

  Top findings:
    1. Validation loop installed but not recording — fix immediately (Phase 2, Story 3).
    2. Dashboard does not reflect the product loop — users cannot see what is
       monitored, missing, or needs action without navigating multiple pages.
    3. Packaged desktop untested — silent failure risk for the primary distribution path.
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

**Outcomes (commit 654e50e):**
- `radar-ui/src/lib/apiClient.ts` — typed fetch wrapper; `initApiClient()` resolves Electron sidecar URL via IPC before first render; all 42+ fetch calls migrated.
- `audit_event` table (migration 030) + `GET/POST /v1/audit-events`.
- App.tsx `useAuth` hook uses api client; sidebar version bumped to v0.2.0.

*Support agents applied: `@agent-refactoring-catalog-advisor`, `@agent-pragmatic-programmer`*

---

### Phase 2 — Product Readiness Loop

**Execution mode:** DELIVERY
**Target:** v0.2.1

**Agreed scope:**
- Add `GET /v1/readiness`.
- Wire the dashboard around "what is monitored, what is missing, what changed, what needs action."
- Improve empty states and setup guidance.
- Make the core drift workflow feel intentional end to end.

**Stories:**

1. **Readiness endpoint** — `GET /v1/readiness` returns a structured checklist: DB connected, migrations current, at least one service registered, at least one diff recorded, at least one consumer registered. Each item has `status` (ok/missing/warn) and a `hint` pointing to the UI action that resolves it.

2. **Dashboard rebuilt around the product loop** — homepage replaces generic summary cards with four zones: what is monitored (service count, consumer count), what is missing (readiness checklist items with hints), what changed recently (last 5 diffs with severity badges), what needs action (unacknowledged breaking changes, active policy blocks).

3. **Instrument core flows** — write `audit_event` records for: `diff.created` (in `diffs.rs`), `consumer.registered` (in `consumers.rs`), `csv_run.started` (in `csv_runner.rs`). This closes the validation loop installed in Phase 1.

4. **Empty state improvements** — Services, Diffs, Consumers, and Catalog Sources pages: replace generic empty messages with contextual cards stating what the page does, the prerequisite action, and the specific next step (link or CLI command).

**DoD additions:**
- `GET /v1/readiness` integration test covers each checklist item state.
- Dashboard readiness zones render correctly in empty-state permutations (manual verification).

*Support agents: `@agent-product-owner-coach`, `@agent-lean-thinking-advisor`, `@agent-observability-advisor`*

---

### Phase 3 — Async Operations Hardening

**Execution mode:** DELIVERY
**Target:** v0.3.0

**Agreed scope:**
- Do not build a generic job platform.
- CSV Runner is the reference implementation for consistent async operation behavior.
- Apply the same pattern individually to: scheduled scans, webhooks, and release notes generation.
- Each should have: consistent status semantics, history, cancellation where relevant, retry behavior, audit records, retention, and useful failure reporting.

**Reference implementation — CSV Runner (complete as of v0.2.0):**

| Property | Implementation |
|---|---|
| Status | `pending` → `running` → `completed` / `completed_with_failures` / `failed` / `cancelled` |
| History | Paginated list of past runs in the UI |
| Cancellation | Checked at row granularity, before each retry sleep |
| Retry | Safe methods always retry; unsafe only with `enable_retry: true` |
| Audit | Job start/end written to `audit_event` |
| Retention | Purged by 1-hour background job after configurable window |
| Failure reporting | Per-row `error` + `error_count` on job; amber badge for partial failures |

**Stories:**

1. **Webhooks** — add per-delivery `status` (`queued`/`delivered`/`failed`), `completed_with_failures` on the webhook record when any delivery fails, failure reason visible in SettingsPage delivery history. Add audit records for delivery attempts.

2. **Scheduled scans** — add `last_run_status` + `last_run_error` + `last_run_at` columns; surface in the list UI; retain run history entries under the same retention window as CSV runs. Add audit records for run start/end.

3. **Release notes generation** — replace fire-and-forget with an async job row; add `GET /v1/release-notes/:id/generate-status`; DiffDetailPage polls until complete or shows inline error. Add audit record for generation start/complete.

4. **In-process mock HTTP server** — shared test helper that spawns a local Axum server returning configurable status codes; replaces the `93.184.216.34` IP literal workaround in CSV Runner, scan, and webhook tests.

*Support agents: `@agent-stability-pattern-advisor`, `@agent-unit-test-coach`, `@agent-test-architecture-advisor`*

---

### Phase 4 — Security Hardening

**Execution mode:** HARDENING
**Target:** v0.3.1

**Agreed scope:**
- Add simple host allowlist / network policy first.
- Normalize SSRF checks into reusable platform code (currently duplicated between CSV runner and scan executor).
- Immediate guarantee: never return raw secrets, never log them.
- Scope encryption at rest as deployment/enterprise work unless SQLCipher or Postgres column encryption becomes a deliberate architectural choice — record the decision either way as an ADR.

**Stories:**

1. **Host allowlist** — `RADAR_ALLOWED_HOSTS` env var (comma-separated glob patterns, e.g. `*.internal,api.github.com`). When set, outbound HTTP in CSV runner and scan executor is blocked unless the resolved hostname matches. Default: empty (no restriction beyond SSRF guard).

2. **Normalize SSRF guard** — extract `is_ssrf_blocked(url)` from `csv_runner.rs` into `radar_api::utils::ssrf`; both CSV runner and scan executor call the same function. Add tests for redirect bypass (already blocked by `redirect::Policy::none()`) and IPv6 literals.

3. **Secret masking** — bearer tokens and API keys in `audit_event.meta` redacted to `[REDACTED]` before insert; sandbox env `bearer_token` field confirmed not returned in `GET /v1/sandbox-envs` response body (verify GET path, not just the PUT guard).

4. **Encryption at rest — ADR-003** — document the decision: "SQLite deployments rely on OS filesystem encryption (FileVault/BitLocker); Postgres relies on infrastructure-level storage encryption. No SQLCipher dependency introduced. Column-level encryption deferred until a specific compliance requirement names it." No code change.

5. **STRIDE test coverage** — threat model (facilitated, not automated) documents one test per HIGH threat asserting the mitigation holds. Minimum: injection, SSRF redirect, auth bypass, information disclosure via error body.

**DoD additions (HARDENING mode):**
- `cargo audit` returns no HIGH or CRITICAL CVEs.
- HIGH threats have a passing test for their mitigation.
- CI asserts secrets do not appear in `audit_event` rows or API responses (grep check in test suite).

*Support agents: `@agent-threat-model-facilitator`, `@agent-stride-threat-analyzer`, `@agent-secure-design-reviewer`, `@agent-privacy-threat-modeler`*

---

### Phase 5 — Test and Release Maturity

**Execution mode:** HARDENING → DELIVERY
**Target:** v0.4.0

**Agreed scope:**
- Add focused Playwright journeys (prioritized over frontend unit tests — Playwright will catch more product breakage for this app).
- For CSV Runner, explicitly include a spawned local echo/test API server in CI.
- Add backend integration tests around org scoping, security boundaries, and async state transitions.
- Add smoke coverage for packaged desktop mode, not just Vite dev mode.

**Stories:**

1. **Playwright E2E journeys** — 5 golden paths tied to the maturity target: (a) register service → upload spec → compare → view diff with blast radius; (b) register consumer → subscribe → blast radius shows that consumer; (c) CSV run: upload CSV → configure template → run → inspect results → export failed rows; (d) register webhook → test fire → delivery appears in history; (e) playground: paste two specs → inline diff. Each journey runs against a live `radar-api` with a test SQLite DB.

2. **CSV Runner CI with echo server** — `cargo test -p radar-api` starts an in-process Axum echo server on a random port (built in Phase 3, Story 4). CSV run tests use `http://127.0.0.1:{port}/echo`. Verifies retry logic, cancellation, body capture, and `completed_with_failures` status without network access.

3. **Backend integration tests: org scoping, security boundaries, async state transitions** — assertions: (a) org A data is not returned to org B requests; (b) unauthenticated requests to auth-required endpoints return 401, not 500; (c) async job state machine is enforced (`running` cannot revert to `pending`; `completed` cannot revert to `running`).

4. **Packaged desktop smoke tests** — `pnpm --filter radar-desktop dist` produces the installer; a smoke test script launches the packaged Electron binary, waits for the sidecar health check at `http://127.0.0.1:17380/health`, and asserts 200. Runs as a separate CI job on tagged releases only.

5. **Automated release pipeline** — GitHub Actions: on push of `v*` tag → `cargo build --release -p radar-api` → `pnpm build:ui` → `electron-builder` → upload `.exe`/`.dmg` as release artifacts → create GitHub Release with matching CHANGELOG section.

*Support agents: `@agent-acceptance-test-designer`, `@agent-deployment-pipeline-designer`, `@agent-evolutionary-architecture-advisor`, `@agent-architectural-decision-recorder`*

---

## 30 / 60 / 90-DAY BACKLOG

*Concrete deliverables, in order. Each item maps to a Phase story above.*

### Days 1–30 (Phase 2 — Product Readiness Loop)

- [ ] `GET /v1/readiness` endpoint with integration test
- [ ] Dashboard: four readiness zones (monitored / missing / changed / needs action)
- [ ] Instrument `diff.created`, `consumer.registered`, `csv_run.started` into `audit_event`
- [ ] Empty state cards: Services, Diffs, Consumers, Catalog Sources

### Days 31–60 (Phase 3 — Async Operations Hardening)

- [ ] Webhooks: per-delivery status, `completed_with_failures`, failure reason in UI, audit records
- [ ] Scheduled scans: `last_run_status` + `last_run_error` + `last_run_at`, history UI, retention, audit records
- [ ] Release notes generation: async job row, status endpoint, DiffDetailPage polling, audit record
- [ ] In-process mock HTTP server for async tests (shared helper; replaces IP literals)

### Days 61–90 (Phase 4 — Security Hardening)

- [ ] `RADAR_ALLOWED_HOSTS` host allowlist
- [ ] SSRF guard extracted to `radar_api::utils::ssrf`; tests for all bypass vectors
- [ ] Secret masking: `audit_event.meta` redaction; `GET /v1/sandbox-envs` response verified
- [ ] ADR-003: encryption at rest decision documented
- [ ] STRIDE coverage: one test per HIGH threat

### Post-90 days (Phase 5 — Test and Release Maturity)

- [ ] Playwright: 5 golden-path journeys
- [ ] CSV Runner echo server in CI
- [ ] Org-scoping and security-boundary integration tests
- [ ] Packaged desktop smoke test (CI, tagged releases only)
- [ ] Automated release pipeline (tag → GitHub Release)

---

## AGENT SELECTION SUMMARY

| Phase | Deliverable | Support agents |
|---|---|---|
| 1 (done) | apiClient.ts, audit_event table | `refactoring-catalog-advisor`, `pragmatic-programmer` |
| 2 | Readiness endpoint, dashboard, usage events, empty states | `product-owner-coach`, `lean-thinking-advisor`, `observability-advisor` |
| 3 | Consistent async behavior: webhooks, scans, release notes | `stability-pattern-advisor`, `unit-test-coach`, `test-architecture-advisor` |
| 4 | Host allowlist, normalized SSRF, secret masking, ADRs | `threat-model-facilitator`, `stride-threat-analyzer`, `secure-design-reviewer`, `privacy-threat-modeler` |
| 5 | Playwright, echo server, org-scoping tests, desktop smoke, release pipeline | `acceptance-test-designer`, `deployment-pipeline-designer`, `evolutionary-architecture-advisor`, `architectural-decision-recorder` |

All agents sourced from `C:\Projects\ClaudeExtras\`.

---

## TECHNICAL DEBT REGISTER

| ID | Artifact | Type | Cause | Principal (est.) | Interest (est.) | Decision |
|---|---|---|---|---|---|---|
| TD-01 | No ADRs | Architecture | Speed during EPIC delivery | 2d | 0.5d/sprint | Pay in Phase 5 |
| TD-02 | No fitness functions | Architecture | CI not extended past lint/test | 1d | 0.2d/sprint | Pay in Phase 5 |
| TD-03 | Webhook/scan/release-note async inconsistency | Architecture | CSV Runner was first; others deferred | 2d | 0.5d/incident | Pay in Phase 3 |
| TD-04 | SSRF guard duplicated | Code | Organic growth | 0.5d | 0.1d/sprint | Pay in Phase 4 |
| TD-05 | Frontend 0% unit tests | Code | No test runner configured | 2d | 0.3d/sprint | Pay in Phase 5 |
| TD-06 | No readiness model or intentional empty states | Infra | Not a focus during EPIC delivery | 1d | Onboarding friction | Pay in Phase 2 |
| TD-07 | Manual release | Infra | No release automation | 1d | 0.5d/release | Pay in Phase 5 |
| TD-08 | No threat model | Infra | Security deferred intentionally | 2d | Risk (unquantified) | Pay in Phase 4 |
| TD-09 | org_id = "default" | Architecture | Multi-tenancy deferred intentionally | 5d | Shotgun surgery when enabled | Accept for now |
| TD-10 | IP-literal test URLs | Infra | Offline CI workaround | 0.5d | Flaky in some CI envs | Pay in Phase 3 |

**Total estimated principal:** ~17d (all figures are estimates, not measured)
**Estimated recurring interest:** ~2.1d/sprint
**Estimated debt ratio:** ~10% of sprint capacity

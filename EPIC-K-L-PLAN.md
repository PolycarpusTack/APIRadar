# API Radar — EPIC K & L Plan

> **Framework:** Backlog Builder v5.1 + GPM v2.1 + Core Specification v1
> **Execution mode:** DELIVERY
> **Preceding EPICs:** A–J complete (enterprise feature set + Non-Technical UX)
> **Scope:** EPIC K (Signals & Integrations) · EPIC L (CSV Runner MVP) · EPIC L+ (stub)
> **Generated:** 2026-05-26

---

## Readiness Decision

**Solution Design Quality Gate**

| Dimension | Score | Notes |
|-----------|-------|-------|
| Clarity | 3/3 | Use cases, UX flows, and API designs are fully specified |
| Feasibility | 3/3 | All components fit the current axum/Rust/SQLite/Postgres + React/Electron stack |
| Completeness | 2/3 | EPIC L+ (full CSV Runner phase) is intentionally stubbed — expand after L retro |

**Total: 8/9 → PROCEED**

**Highest risks:**
- Webhook delivery reliability (in-process Tokio vs. external queue) — mitigated by DB-backed delivery log with retry
- SSRF surface on server-side URL fetching (Scheduled Scan) — mitigated by RFC1918 block list in resolver
- CSV Runner scope creep into load testing — mitigated by hard concurrency cap and explicit labelling

---

## Critical Gaps

None blocking. Bounded assumptions recorded below.

---

## Domain Glossary (Additions for K & L)

> All existing terms from `DEVELOPMENT_PLAN.md` remain in force. The terms below extend the glossary for these EPICs only.

| Term | Definition |
|------|-----------|
| **Signal** | Any outbound notification emitted by API Radar — Webhook, email Digest, or chat message |
| **Webhook** | An HTTP callback URL registered by an Operator to receive Push Notifications when named Events occur in their Organisation |
| **Event** | A named, immutable occurrence in API Radar that may trigger Signal delivery (e.g. `diff.created`, `breaking_change.detected`) |
| **Delivery** | A single attempt to POST a Webhook payload to a registered URL; has status `pending`, `delivered`, `failed` |
| **Digest** | A periodic (weekly) aggregated summary of drift activity, delivered by email to configured recipients |
| **Scheduled Scan** | An automated drift check triggered on a cron schedule against a stored spec URL rather than by a CI push |
| **Share Token** | An opaque, read-only token granting unauthenticated access to a single Diff detail view |
| **CSV Runner** | A Playground extension that executes a parameterised HTTP request once per row of an uploaded CSV file |
| **Run** | A single execution of the CSV Runner; contains one Row Result per CSV row |
| **Row Result** | The outcome (HTTP status, duration, error, response preview) of executing one iteration of a Run |
| **Template** (Runner) | A parameterised HTTP request where `{{column_name}}` placeholders are resolved from CSV column values per row |
| **Variable** (Runner) | A named placeholder (`{{name}}`) in a Template; resolved from the matching CSV column header |
| **Iteration** | One execution of a Template against one CSV row, producing one Row Result |

---

## Assumptions Ledger

| # | Assumption | Impact | Verification point |
|---|-----------|--------|-------------------|
| A-1 | Webhook delivery is in-process (Tokio task) — no external queue | **HIGH** — limits throughput to ~100 deliveries/min at concurrency 10; sufficient for desktop + small-team web deployment | K-1 retrospective |
| A-2 | Email digest uses SMTP with operator-configured credentials (no hosted email service) | Med — desktop users will typically skip this; web deployment operators configure it | K-6 |
| A-3 | Scheduled Scan polls at minimum 15-minute intervals to avoid overloading target hosts | Med — power users may want shorter intervals | K-4 retrospective |
| A-4 | CSV Runner is browser-side in MVP; no server-side worker or queue | **HIGH** — runs don't survive page refresh; results not persisted to DB | L retro → EPIC L+ |
| A-5 | CSV Runner max rows = 500 for MVP (browser memory) | Med | L-4 |
| A-6 | Public Diff permalink is read-only and does not expose org_id in the URL | Med | K-5 |

---

## ADR Register

| ID | Decision | Rationale |
|----|---------|-----------|
| ADR-K-1 | Webhook delivery: in-process Tokio task + DB delivery log | External queue adds infrastructure not yet present; DB log provides retry and audit without it. Revisit at scale. |
| ADR-K-2 | Webhook signature: HMAC-SHA256 of raw payload body with `X-Radar-Signature-256` header | Industry standard (matches GitHub); allows receivers to verify authenticity without shared session |
| ADR-K-3 | Scheduled Scan: store spec URL + last known spec text; diff against fetched text | Reuses existing diff infrastructure; no new spec storage needed |
| ADR-K-4 | Public Diff permalink: short Share Token in URL, not diff ID directly | Avoids exposing sequential or guessable IDs; token can be revoked |
| ADR-L-1 | CSV Runner MVP: browser-side fetch loop, React state for results | Eliminates all infrastructure questions; establishes UX; backend persistence added in EPIC L+ |
| ADR-L-2 | Variable syntax: `{{column_name}}` — exact match to existing Postman convention | Reduces cognitive load; consistent with generate-tests and Playground tooling already in use |

---

## SLO Definitions

**EPIC K**
- `webhook-delivery — p95 delivery latency < 30s over 1h rolling window`
- `scheduled-scan — start delay < 5min of scheduled time, 99% over 24h`
- `public-diff — p99 time-to-first-byte < 500ms (no auth overhead)`

**EPIC L**
- `csv-runner — row execution overhead < 200ms per row above raw HTTP latency`

---

---

# EPIC K — Signals & Integrations

**Objective:** Make API Radar push information out so teams receive drift alerts in their existing tools without checking the dashboard.

**Tracer Bullet?:** YES (K-1 is the tracer bullet)

**Mode:** DELIVERY

**Definition of Done:**
- At least one Webhook can be registered, receives a `diff.created` payload within 30 seconds of a diff being posted, and the Delivery is recorded in the audit log
- The public Diff permalink renders the full blast-radius report without authentication
- Scheduled Scan detects a spec change and fires a Webhook automatically

**Business Value:** Teams using API Radar currently learn about drift only when they manually check the dashboard or run the CLI in CI. Signals close this gap and make drift detection ambient — reducing mean-time-to-awareness of breaking changes.

**Risk Assessment:**
- **High: Webhook abuse (SSRF via user-controlled URL)** — mitigated by allowing only `https://` URLs and blocking RFC1918 targets at delivery time
- **Med: Tokio-based delivery fails silently** — mitigated by DB delivery log and dead-letter visibility in Settings UI
- **Med: Scheduled Scan overwrites spec versions on each run** — mitigated by storing previous hash and only creating a diff when hash changes

**Runbook:** See `docs/runbook.md` — add section K after EPIC K Phase 4

---

## K-1 · Webhook Registration & Delivery Foundation ⚡ TRACER BULLET

**Persona Narrative:** As an Operator I want to register a Webhook URL so that my tooling receives an HTTP notification whenever a new Diff is created.

**Business Value:** 3 (High) | **Priority Score:** 5

**Size:** L

**INVEST:** I✓ N✓ V✓ E✓ S✓ T✓

**DoR Status:** READY

**Acceptance Criteria:**
```gherkin
Given a valid HTTPS URL is submitted to POST /v1/webhooks
When a new Diff is created in the same org
Then a POST is made to the URL within 30 seconds
  And the payload contains diff_id, service_id, breaking_count, changes_count, created_at
  And the request carries X-Radar-Signature-256: sha256=<hmac>
  And a webhook_delivery row is written with status='delivered'

Given the target URL returns HTTP 5xx or times out
When delivery is attempted
Then the system retries up to 3 times with exponential backoff (1s, 4s, 16s)
  And after 3 failures the delivery row status becomes 'failed'

Given an invalid URL (non-HTTPS, RFC1918 target, or missing host)
When POST /v1/webhooks is called
Then HTTP 422 is returned with a descriptive error
  And no webhook_delivery row is created

Given GET /v1/webhooks is called
Then all webhooks for the caller's org_id are returned
  And secret values are masked (only first 4 chars visible)
```

**Idempotency Strategy:** Webhook registration is idempotent on `(org_id, url, events[])` — re-posting the same URL+events upserts rather than duplicating.

**External Dependencies:** None (in-process delivery, no external queue)

**Technical Debt Considerations:** In-process delivery is TD-K-1 — acceptable for MVP; becomes a liability at high Webhook volume. Review at 10k deliveries/day.

---

### K-1-T1 · DB migrations — webhook + webhook_delivery tables

**Hat:** FEATURE
**Goal:** Add webhook and webhook_delivery schema to support Webhook registration and Delivery audit

**TDD Execution Order:**
1. Write migration tests (table existence, column types, FK constraints) using existing test harness
2. Write migrations 014 and 015
3. Verify both SQLite and PostgreSQL compatibility (TEXT ids, no SERIAL)

**Deliverables:**
- `radar-api/migrations/014_webhooks.sql` — webhook table
- `radar-api/migrations/015_webhook_deliveries.sql` — webhook_delivery table

**Schema:**
```sql
-- 014
CREATE TABLE webhook (
  id          TEXT PRIMARY KEY,
  org_id      TEXT NOT NULL DEFAULT '',
  url         TEXT NOT NULL,
  events      TEXT NOT NULL DEFAULT 'diff.created',
  secret      TEXT NOT NULL,
  active      INTEGER NOT NULL DEFAULT 1,
  created_at  TEXT NOT NULL
);

-- 015
CREATE TABLE webhook_delivery (
  id          TEXT PRIMARY KEY,
  webhook_id  TEXT NOT NULL REFERENCES webhook(id) ON DELETE CASCADE,
  event       TEXT NOT NULL,
  payload     TEXT NOT NULL,
  status      TEXT NOT NULL DEFAULT 'pending',
  attempt     INTEGER NOT NULL DEFAULT 0,
  error       TEXT,
  delivered_at TEXT
);
```

**Pull Gate:** Existing migrations 001–013 applied cleanly
**Unblocks:** K-1-T2
**Confidence:** High

---

### K-1-T2 · Webhook CRUD endpoints

**Hat:** FEATURE
**Goal:** Implement POST, GET, DELETE /v1/webhooks with SSRF protection and secret masking

**TDD Execution Order:**
1. Tests: register valid webhook → 201; re-register same URL+events → 200 (upsert); RFC1918 URL → 422; HTTP URL → 422; DELETE → 204; list returns masked secret
2. Implement `radar-api/src/webhooks.rs` handlers
3. Add routes to `lib.rs`

**Key implementation notes:**
- Validate URL: must be `https://`, host must resolve to non-RFC1918 IP (block 10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16, 127.0.0.0/8, 169.254.0.0/16)
- Generate secret: `Uuid::new_v4().to_string()` — returned once on create, masked thereafter
- `events` stored as comma-separated string for SQLite compat

**Pull Gate:** K-1-T1 migrations applied
**Unblocks:** K-1-T3
**Confidence:** High

---

### K-1-T3 · Webhook delivery engine

**Hat:** FEATURE
**Goal:** Fire Webhook deliveries in a background Tokio task on diff.created event with HMAC signing, retry, and DB audit

**TDD Execution Order:**
1. Unit tests: HMAC signature generation, retry backoff calculation, payload serialisation
2. Integration test: create diff → assert webhook_delivery row with status='delivered' (use local echo server)
3. Implement delivery task spawned from `create_diff` and `compare_specs` handlers

**Implementation:**
```rust
// Called after any diff is persisted
pub async fn dispatch_diff_event(pool: &AnyPool, diff_id: &str, org_id: &str) {
    // fetch active webhooks for org
    // for each: tokio::spawn delivery task
    // delivery task: sign payload, POST with timeout 10s, retry 3×, update delivery row
}
```

**HMAC:** `X-Radar-Signature-256: sha256=hex(HMAC-SHA256(secret, raw_body))`

**Pull Gate:** K-1-T2 routes returning 201
**Unblocks:** K-1-T4
**Confidence:** Med (integration test needs local server; use `axum_test` or mock)

---

### K-1-T4 · Delivery log UI in Settings

**Hat:** FEATURE
**Goal:** Show registered Webhooks and recent Delivery history in SettingsPage with test-fire button

**TDD Execution Order:**
1. No unit tests for UI; add E2E smoke check (webhook list renders)
2. Implement webhook section in `SettingsPage.tsx`: list, status badges (delivered/failed/pending), delete, "Send test event" button
3. POST /v1/webhooks/{id}/test endpoint (fires a `ping` event with synthetic payload)

**Pull Gate:** K-1-T3 delivery engine returning 200
**Unblocks:** K-2
**Confidence:** High

---

## K-2 · Slack Notification Template

**Persona Narrative:** As an Operator I want drift events to post a formatted message to our Slack channel so that the team sees breaking changes without checking the dashboard.

**Business Value:** 3 (High) | **Priority Score:** 4

**Size:** M

**INVEST:** I✓ N✓ V✓ E✓ S✓ T✓

**DoR Status:** READY

**Acceptance Criteria:**
```gherkin
Given a Slack Incoming Webhook URL is registered with event_type='slack'
When a diff with 2 breaking changes is created
Then Slack receives a Block Kit message containing service name, breaking count, and a "View Diff" link

Given the Slack URL returns 200 OK
Then the webhook_delivery row status is 'delivered'

Given POST /v1/webhooks/{id}/test is called for a Slack webhook
Then a test Block Kit message is POSTed to the Slack URL
```

**Notes:** Slack Incoming Webhooks do not require HMAC. Add `type` field to webhook table: `generic` (default, HMAC signed) | `slack` (Block Kit payload, no signature). Generic webhooks remain the primary design.

---

### K-2-T1 · Slack webhook type + Block Kit payload

**Hat:** FEATURE
**Goal:** Add slack webhook type, render Block Kit payload for diff events

**TDD Execution Order:**
1. Unit test: given diff summary struct, Block Kit payload JSON matches expected shape
2. Add `type` column to webhook table (migration 016: `ALTER TABLE webhook ADD COLUMN type TEXT NOT NULL DEFAULT 'generic'`)
3. In delivery engine: branch on `type` — skip HMAC, use Block Kit template for Slack

**Block Kit template:**
```json
{
  "blocks": [
    { "type": "header", "text": { "type": "plain_text", "text": "⚡ API Drift Detected — {{service_name}}" }},
    { "type": "section", "fields": [
      { "type": "mrkdwn", "text": "*Breaking Changes*\n{{breaking_count}}" },
      { "type": "mrkdwn", "text": "*Total Changes*\n{{changes_count}}" }
    ]},
    { "type": "actions", "elements": [
      { "type": "button", "text": { "type": "plain_text", "text": "View Diff" }, "url": "{{dashboard_url}}" }
    ]}
  ]
}
```

**Pull Gate:** K-1-T3 delivery engine green
**Unblocks:** K-3
**Confidence:** High

---

## K-3 · Scheduled Spec Scan

**Persona Narrative:** As a Platform Engineer I want API Radar to automatically check a service's spec URL on a schedule so that drift is detected even when no CI run has occurred.

**Business Value:** 3 (High) | **Priority Score:** 4

**Size:** L

**INVEST:** I✓ N✓ V✓ E✓ S✓ T✓

**DoR Status:** READY

**Acceptance Criteria:**
```gherkin
Given a scheduled_scan is created for service X with spec_url and interval_minutes=60
When 60 minutes pass
Then radar-api fetches the spec_url and compares against the last stored spec text
  And if the spec changed: a Diff is created (reusing compare_specs logic)
  And Webhooks for diff.created are fired as normal
  And last_run_at and last_spec_hash are updated

Given the spec_url returns HTTP 4xx/5xx or times out
Then no Diff is created
  And the scan is logged with status='fetch_error'
  And next run is scheduled normally

Given the spec has not changed (hash unchanged)
Then no Diff is created
  And last_run_at is updated

Given interval_minutes < 15 is submitted
Then HTTP 422 is returned (minimum interval enforced)
```

**Idempotency:** Scan identified by `(org_id, service_id, spec_url)` — upsert on conflict.

---

### K-3-T1 · scheduled_scan table + CRUD API

**Hat:** FEATURE
**Goal:** Persist scheduled scan configuration and expose management endpoints

**TDD Execution Order:**
1. Tests: create scan → 201; interval < 15 → 422; list; delete
2. Migration 017: `scheduled_scan` table (id, org_id, service_id, spec_url, format, interval_minutes, last_run_at, last_spec_hash, active)
3. Handlers in `radar-api/src/scans.rs`, routes in lib.rs

**Pull Gate:** Migrations 001–016 clean
**Unblocks:** K-3-T2
**Confidence:** High

---

### K-3-T2 · Scan scheduler background task

**Hat:** FEATURE
**Goal:** Tokio background task that polls due scans, fetches spec URLs (with SSRF protection), and triggers diffs

**TDD Execution Order:**
1. Unit tests: hash comparison, due-scan query (mock time), SSRF block
2. Integration test: create scan with 1-minute interval → advance mock time → assert diff created
3. Start scheduler in `main.rs` on `app.whenReady()` equivalent (startup)

**Key logic:**
```rust
loop {
    sleep(Duration::from_secs(60)).await;
    let due = fetch_due_scans(&pool).await; // WHERE last_run_at < NOW() - interval_minutes
    for scan in due {
        tokio::spawn(run_scan(pool.clone(), scan));
    }
}
```

**SSRF:** Reuse same RFC1918 block logic from K-1-T2 webhook validation.

**Pull Gate:** K-3-T1 table exists
**Unblocks:** K-4
**Confidence:** Med (integration test with time mocking)

---

## K-4 · Public Diff Permalink

**Persona Narrative:** As a Developer I want to share a link to a Diff detail page that anyone can open without logging in so that I can include it in PR descriptions and Slack messages.

**Business Value:** 2 (Med) | **Priority Score:** 3

**Size:** M

**INVEST:** I✓ N✓ V✓ E✓ S✓ T✓

**DoR Status:** READY

**Acceptance Criteria:**
```gherkin
Given a Diff exists with diff_id=X
When GET /v1/diffs/X returns the diff
Then the response includes a share_token

Given the share_token is known
When a browser navigates to /share/{share_token} (no auth)
Then the full DiffDetailPage renders including changes, blast radius, and service name

Given the share_token does not exist or is revoked
When the URL is visited
Then HTTP 404 is returned with a plain error page

Given the DiffDetailPage share view
Then no sidebar, no navigation, and no org-level data is shown
  And a "Sign in to see full context" CTA is visible
```

---

### K-4-T1 · Share token generation and lookup

**Hat:** FEATURE
**Goal:** Add share_token to diff table; expose GET /share/{token} public endpoint returning diff JSON

**TDD Execution Order:**
1. Tests: generate token on diff creation; GET /share/{valid_token} → 200; invalid token → 404; no org_id in response
2. Migration 018: `ALTER TABLE diff ADD COLUMN share_token TEXT UNIQUE`
3. Back-fill with `Uuid::new_v5(NAMESPACE_URL, diff_id)` on read if null
4. Public route (outside auth middleware): `GET /share/:token`

**Pull Gate:** Diff table accessible
**Unblocks:** K-4-T2
**Confidence:** High

---

### K-4-T2 · ShareDiffPage React component

**Hat:** FEATURE
**Goal:** Stripped-down DiffDetailPage that loads via share token, shows no sidebar, no auth-gated data

**TDD Execution Order:**
1. No unit tests; add smoke test: navigate to /share/bad-token → shows 404 state
2. Route `/share/:token` in App.tsx → `ShareDiffPage`
3. Reuse DiffDetailPage logic but replace fetch with `/share/${token}`, hide sidebar, add "View in API Radar" CTA

**Feature Flag:** `RADAR_PUBLIC_SHARE` (default: on) — operators can disable public sharing at the server level via env var

**Pull Gate:** K-4-T1 endpoint returning 200
**Unblocks:** K-5
**Confidence:** High

---

## K-5 · Weekly Email Digest

**Persona Narrative:** As a Team Lead I want to receive a weekly email summarising drift activity so that I stay informed without logging into the tool.

**Business Value:** 2 (Med) | **Priority Score:** 3

**Size:** M

**INVEST:** I✓ N✓ V✓ E✓ S✓ T✓

**DoR Status:** READY

**Acceptance Criteria:**
```gherkin
Given SMTP is configured and digest recipients are set in Settings
When Monday 08:00 UTC arrives
Then one email is sent per configured org containing: total diffs this week, breaking change count, top 3 affected services, links to open diffs

Given POST /v1/notifications/digest/preview is called
Then the email HTML is returned as a response body (no email sent)

Given SMTP credentials are not configured
Then no email is attempted and the digest task logs a skip event
```

**External Dependencies:** SMTP server (operator-configured). No hosted service dependency.

---

### K-5-T1 · Digest scheduler + SMTP delivery

**Hat:** FEATURE
**Goal:** Background Tokio task fires weekly digest; SMTP delivery via lettre; preview endpoint

**TDD Execution Order:**
1. Unit tests: digest data aggregation query, HTML template render, SMTP config validation
2. Integration test: POST /v1/notifications/digest/preview returns valid HTML with correct counts
3. Add `lettre` to radar-api Cargo.toml; implement digest task

**Note:** `lettre` is the standard async Rust SMTP crate. Add to workspace dependencies.

**Pull Gate:** Settings table can store SMTP config (add to migration if needed)
**Unblocks:** K-6
**Confidence:** Med (SMTP integration test needs mock SMTP; use `fake-smtp` or mailhog in CI)

---

## K-6 · GitHub App Status Check Polish

**Persona Narrative:** As a CI/CD Engineer I want the GitHub status check to update when a diff is re-acknowledged so that the PR gate reflects the latest state without re-running CI.

**Business Value:** 2 (Med) | **Priority Score:** 3

**Size:** S

**INVEST:** I✓ N✓ V✓ E✓ S✓ T✓

**DoR Status:** READY

**Acceptance Criteria:**
```gherkin
Given a diff is posted via CLI with --post-comment and a breaking change exists
When the diff is acknowledged in the UI
Then a new GitHub status check is posted updating the state to 'success' with description 'Acknowledged'

Given GITHUB_TOKEN is not in environment
Then acknowledgement proceeds normally and no status check attempt is made
```

---

### K-6-T1 · GitHub status check on acknowledgement

**Hat:** FEATURE
**Goal:** POST GitHub status check when a diff acknowledgement is created, if the diff has a PR URL

**TDD Execution Order:**
1. Unit test: given diff with pr_url, acknowledgement triggers status check POST (mock GitHub API)
2. In `acknowledgements.rs` create handler: after insert, spawn Tokio task to POST GitHub status
3. Parse `pr_url` to extract owner/repo/sha; use `GITHUB_TOKEN` env var

**Pull Gate:** Acknowledgements endpoint functional
**Unblocks:** K-SMOKE
**Confidence:** Med (GitHub API mocking)

---

## K-SMOKE · EPIC K End-to-End Smoke Test

**Hat:** FEATURE
**Goal:** Automate the critical EPIC K journeys as an integration test suite

**TDD Execution Order:**
1. Write smoke scenarios before any K story (as executable specs against a local server)
2. Run against dev server after each story merge

**Scenarios:**
1. Register Webhook → create diff via POST → assert webhook_delivery.status='delivered' within 5 seconds
2. Create Scheduled Scan → simulate due time → assert diff created
3. GET /share/{token} (no auth) → 200 with diff data
4. POST /v1/notifications/digest/preview → 200, HTML contains expected service name

**Pull Gate:** K-1 through K-6 merged
**Unblocks:** END OF EPIC K SEQUENCE
**Confidence:** High

---

---

# EPIC L — CSV Runner MVP (Playground Extension)

**Objective:** Let users upload a CSV in the Playground, bind columns to `{{variable}}` placeholders in their request template, preview resolved requests, and execute them sequentially with a per-row results table.

**Tracer Bullet?:** YES (L-1 is the tracer bullet for this EPIC)

**Mode:** DELIVERY

**Definition of Done:**
- A user can upload a 100-row CSV, see column-to-variable auto-mapping, preview the first 5 resolved requests, run all rows sequentially, and export results as CSV
- The feature is behind the `RADAR_CSV_RUNNER` feature flag (default: on)
- No new Rust endpoints or database tables are required for MVP

**Business Value:** Operational and QA users can perform bulk API operations without writing scripts, expanding API Radar's reach beyond engineering teams.

**Risk Assessment:**
- **High: Scope creep into load testing** — mitigated by hard cap of 500 rows, concurrency=1 only, explicit "Data Runner" labelling throughout
- **Med: Browser-side execution vs. CORS** — if target APIs lack CORS headers, fetch will fail silently; mitigated by clear error display per row
- **Med: Large CSVs cause browser memory pressure** — mitigated by 500-row and 10MB file limits validated on upload

**SLO Definitions:** N/A for browser-side MVP. Track in L+ when server-side.

**Runbook:** Not required for browser-side MVP (no server components).

---

## L-1 · CSV Upload + Variable Detection ⚡ TRACER BULLET

**Persona Narrative:** As a Solutions Engineer I want to upload a CSV in the Playground and see which columns map to request variables so that I can prepare a bulk run without writing code.

**Business Value:** 3 (High) | **Priority Score:** 5

**Size:** M

**INVEST:** I✓ N✓ V✓ E✓ S✓ T✓

**DoR Status:** READY

**Acceptance Criteria:**
```gherkin
Given the Playground has a request with URL containing {{user_id}}
And headers containing {{api_token}}
When the user uploads a CSV with headers: user_id, api_token, notes
Then the UI shows:
  user_id → mapped (column found)
  api_token → mapped (column found)
  And "notes" is listed as an unused column

Given a CSV with a duplicate header (user_id, user_id)
When uploaded
Then an error is shown: "Duplicate column header: user_id"
  And the mapping screen does not proceed

Given a CSV exceeding 500 rows or 10 MB
When uploaded
Then an error is shown with the specific limit exceeded
```

**Feature Flag:** `RADAR_CSV_RUNNER` (default: `true`)

---

### L-1-T1 · CSV parser utility (browser-side)

**Hat:** FEATURE
**Goal:** Pure TypeScript CSV parser that handles quoted fields, UTF-8 BOM, line breaks in cells, and returns headers + row arrays

**TDD Execution Order:**
1. Unit tests (vitest): standard CSV; quoted commas; quoted newlines; UTF-8 BOM; duplicate headers → error; empty file → error; 501 rows → error
2. Implement `radar-ui/src/lib/csvParser.ts` — no external CSV dependency
3. Export `parseCsv(text: string, maxRows = 500): ParseResult`

**Test data:** small fixture CSVs in `radar-ui/src/lib/__fixtures__/`

**Pull Gate:** vitest passes on existing suite
**Unblocks:** L-1-T2
**Confidence:** High

---

### L-1-T2 · Variable extractor utility

**Hat:** FEATURE
**Goal:** Extract all `{{variable_name}}` placeholders from a Playground request (URL, query params, headers, body) and return a deduplicated list

**TDD Execution Order:**
1. Unit tests: URL with 2 vars; header value with 1 var; JSON body with nested vars; no vars → empty list; malformed `{{` → ignored
2. Implement `radar-ui/src/lib/variableExtractor.ts` — regex `/\{\{([a-zA-Z_][a-zA-Z0-9_]*)\}\}/g`
3. Export `extractVariables(request: PlaygroundRequest): string[]`

**Pull Gate:** L-1-T1 parser green
**Unblocks:** L-1-T3
**Confidence:** High

---

### L-1-T3 · CSV Runner tab in PlaygroundPage

**Hat:** FEATURE
**Goal:** Add "Run with CSV" tab to PlaygroundPage that shows upload button, parsed column list, and auto-mapping table

**TDD Execution Order:**
1. Smoke test: upload CSV → mapping table renders; duplicate header → error banner renders
2. Add tab toggle to `PlaygroundPage.tsx`: `single` | `csv`
3. Implement `CsvRunnerPanel.tsx` — file upload, invoke csvParser, invoke variableExtractor on current request, render mapping table (variable → column status)

**Feature Flag:** render tab only when `RADAR_CSV_RUNNER` env var is truthy (read from window.__RADAR_CONFIG__ or default true)

**Pull Gate:** L-1-T1 and L-1-T2 utilities passing tests
**Unblocks:** L-2
**Confidence:** High

---

## L-2 · Request Preview Table

**Persona Narrative:** As a Solutions Engineer I want to preview the first 10 resolved requests before running them so that I can verify variables are substituted correctly and no sensitive data is exposed unexpectedly.

**Business Value:** 3 (High) | **Priority Score:** 4

**Size:** S

**INVEST:** I✓ N✓ V✓ E✓ S✓ T✓

**DoR Status:** READY

**Acceptance Criteria:**
```gherkin
Given a valid CSV upload with mapping complete
When the user clicks "Preview"
Then a table shows the first 10 rows with: row number, resolved URL, resolved body preview (truncated 200 chars)
  And header values matching patterns (Authorization, token, key, secret, password) are masked as ****

Given a row where a variable is undefined (column missing)
When it appears in the preview
Then the cell shows the unresolved placeholder in red: {{missing_var}}
  And a warning banner states "1 row has unresolved variables"

Given "Run" is clicked with unresolved variables present
Then a confirmation dialog warns the user before proceeding
```

---

### L-2-T1 · Variable resolver + secret masker

**Hat:** FEATURE
**Goal:** Resolve `{{var}}` placeholders in a request template against one CSV row; mask secret-looking header values

**TDD Execution Order:**
1. Unit tests: resolve all vars; unresolved var stays as literal; mask Authorization header; mask header containing 'token', 'key', 'secret', 'password', 'bearer'; non-secret header unchanged
2. Implement `radar-ui/src/lib/variableResolver.ts`
3. Export `resolveRequest(template, row): ResolvedRequest` and `maskSecrets(headers): Record<string, string>`

**Pull Gate:** L-1-T2 extractor green
**Unblocks:** L-2-T2
**Confidence:** High

---

### L-2-T2 · Preview table component

**Hat:** FEATURE
**Goal:** Render preview table inside CsvRunnerPanel showing first 10 resolved requests

**TDD Execution Order:**
1. Smoke test: given 3-row CSV, preview table shows 3 rows with correct URLs
2. Add `PreviewTable.tsx` — calls resolveRequest for rows[0..9]; shows row, method, URL, body preview, unresolved var warnings
3. Add "Preview" button to CsvRunnerPanel that toggles preview section

**Pull Gate:** L-2-T1 resolver green
**Unblocks:** L-3
**Confidence:** High

---

## L-3 · Sequential Execution Engine

**Persona Narrative:** As a Solutions Engineer I want to run all CSV rows against the API and watch results appear row by row so that I know immediately which calls succeeded or failed.

**Business Value:** 3 (High) | **Priority Score:** 5

**Size:** M

**INVEST:** I✓ N✓ V✓ E✓ S✓ T✓

**DoR Status:** READY

**Acceptance Criteria:**
```gherkin
Given a 50-row CSV and a valid request template
When the user clicks "Run Batch"
Then each row is executed sequentially (one in-flight at a time)
  And the results table updates after each row with: status badge, HTTP status, duration_ms
  And a progress indicator shows "23/50 complete"
  And a Cancel button stops the remaining rows at the next row boundary

Given a row returns HTTP 4xx or 5xx
Then it is marked FAILED in the results table
  And execution continues to the next row (stopOnError=false default)

Given the browser tab is closed during execution
Then execution stops (no server-side persistence in MVP)
  And a warning is shown before unload: "A run is in progress — leaving will cancel it"
```

---

### L-3-T1 · Execution engine with cancellation

**Hat:** FEATURE
**Goal:** Async execution loop in React that runs rows sequentially, updates state per row, and supports cancellation

**TDD Execution Order:**
1. Unit tests: 3-row mock execution → 3 results; cancel after row 1 → only 1 result; timeout row → error result
2. Implement `useRunEngine` custom hook: `runAll(rows, request, signal: AbortSignal)` → AsyncIterable of RowResult
3. Each iteration: resolveRequest → fetch with AbortSignal → record {status, httpStatus, durationMs, error}

**Implementation note:** Use `AbortController` — `cancel()` sets signal, next iteration's `fetch` is aborted.

**Pull Gate:** L-2-T1 resolver + L-1-T1 parser green
**Unblocks:** L-3-T2
**Confidence:** Med (AbortController + async iteration pattern needs careful test setup)

---

### L-3-T2 · Progress UI + results table

**Hat:** FEATURE
**Goal:** Render live progress bar and results table updating after each row completes

**TDD Execution Order:**
1. Smoke test: given 3-row run, results table has 3 rows on completion
2. Add `RunProgressPanel.tsx` — progress bar (completed/total), status counts (success/failed), results table with live updates
3. Add `beforeunload` handler warning when run is active

**Pull Gate:** L-3-T1 hook returns results
**Unblocks:** L-4
**Confidence:** High

---

## L-4 · Results Export + Run Summary

**Persona Narrative:** As a Solutions Engineer I want to export the run results as a CSV so that I can share them with stakeholders or load failures into a retry workflow.

**Business Value:** 2 (Med) | **Priority Score:** 3

**Size:** S

**INVEST:** I✓ N✓ V✓ E✓ S✓ T✓

**DoR Status:** READY

**Acceptance Criteria:**
```gherkin
Given a completed run with mixed successes and failures
When the user clicks "Export Results CSV"
Then a CSV file downloads with columns: row_number, http_status, duration_ms, error, url
  And cells beginning with =, +, -, @ are escaped with a leading single-quote

Given the user clicks "Download Failed Rows"
Then a CSV downloads containing only the original input rows that failed
  And all original CSV columns are preserved plus: error, http_status

Given the run summary bar
Then it shows: total, succeeded, failed, avg duration, and total elapsed time
```

---

### L-4-T1 · CSV export utility + formula injection protection

**Hat:** FEATURE
**Goal:** Generate downloadable CSV from run results with formula injection escaping

**TDD Execution Order:**
1. Unit tests: normal row; row starting with '=' → escaped; row with embedded comma → quoted; failed-rows-only filter
2. Implement `radar-ui/src/lib/csvExporter.ts` — `exportResults(results, originalRows): Blob`
3. Wire "Export Results" and "Download Failed Rows" buttons in RunProgressPanel

**Pull Gate:** L-3-T2 results available in state
**Unblocks:** L-SMOKE
**Confidence:** High

---

## L-SMOKE · EPIC L End-to-End Smoke Test

**Hat:** FEATURE
**Goal:** Automate the primary CSV Runner journey in the browser

**TDD Execution Order:**
1. Write Playwright smoke test: upload CSV → preview → run → assert results table row count matches CSV rows → export download triggered

**Smoke scenario (manual fallback):**
1. Open Playground → switch to CSV tab
2. Enter a GET request to `https://httpbin.org/get?id={{id}}`
3. Upload a 5-row CSV: `id\n1\n2\n3\n4\n5`
4. Preview: 5 rows show resolved URLs
5. Run: 5 green results within 30 seconds
6. Export: CSV downloads with 5 rows

**Pull Gate:** L-1 through L-4 merged
**Unblocks:** END OF EPIC L SEQUENCE
**Confidence:** High

---

---

# EPIC L+ — CSV Runner Full Phase (Stub)

> **Status:** NOT STARTED — expand after EPIC L retrospective confirms MVP value
> **Mode:** DELIVERY
> **Trigger:** Expand if ≥3 users request server-side persistence, retries, or workflow mode within 4 weeks of L release

**Objective:** Elevate CSV Runner from browser-side prototype to a durable, server-side execution service with persistence, retries, workflow support, and scheduled runs.

**Rough stories (to be detailed after L retro):**

| ID | Story | Size | Unblocks |
|----|-------|------|---------|
| L+-1 | Run persistence — save Run + Row Results to DB | L | L+-2 |
| L+-2 | Server-side execution worker (Tokio task pool, concurrency 1–10) | XL | L+-3 |
| L+-3 | SSE progress stream — live updates without polling | M | L+-4 |
| L+-4 | Retry failed rows — create child Run from parent's failures | M | L+-5 |
| L+-5 | Assertions — define expected status/body conditions per row | L | L+-6 |
| L+-6 | Workflow mode — multi-step sequence per CSV row with response extraction | XL | L+-7 |
| L+-7 | Scheduled CSV runs — cron trigger + stored template + stored dataset | L | END |

**Infrastructure pre-requisites (PREP stories):**
- PREP-L+-1: Add `csv_run`, `csv_run_row` tables (reuse delivery log pattern from K-1)
- PREP-L+-2: Add worker pool abstraction reusing Tokio task spawning from K-3 scan scheduler
- PREP-L+-3: Add SSE endpoint pattern (single route → EventSource stream)

**ADRs to draft before L+ starts:**
- ADR-L+-1: Worker concurrency model (Tokio semaphore vs. separate task pool)
- ADR-L+-2: Response body storage (DB BLOB vs. object storage) — revisit at 10k rows/day
- ADR-L+-3: SSRF policy for server-side execution (reuse webhook block list from ADR-K-1)

---

---

## Validator Summary

| Check | Status | Notes |
|-------|--------|-------|
| DAG structure | ✓ | Each task has Unblocks + Pull Gate; no cycles |
| EPIC 1 is tracer bullet | ✓ | K-1 (Webhook foundation) is the tracer bullet for EPIC K; L-1 for EPIC L |
| Every task declares Hat | ✓ | All FEATURE |
| TDD order declared | ✓ | All tasks list failing tests → implementation |
| No Two Hats violations | ✓ | No task mixes FEATURE + REFACTORING |
| DoR met or HOLD | ✓ | All stories READY |
| Token budget respected | ✓ | No task exceeds estimated 1,500 LOC |
| Max initial depth ≤ 2 EPICs | ✓ | K and L detailed; L+ is stub |
| SLO definitions present | ✓ | Per EPIC |
| Idempotency declared | ✓ | Webhook upsert; Scheduled Scan upsert |
| ADRs for cross-cutting decisions | ✓ | ADR-K-1 through ADR-L-2 |
| Smoke test story per EPIC | ✓ | K-SMOKE, L-SMOKE |
| Feature flags for user-facing changes | ✓ | `RADAR_PUBLIC_SHARE`, `RADAR_CSV_RUNNER` |
| Assumptions Ledger present | ✓ | 6 entries, 2 flagged HIGH |
| PII consideration | ✓ | CSV rows may contain PII — L-4 export uses truncated preview; noted for L+ data governance |
| Formula injection protection | ✓ | L-4-T1 escapes `=`, `+`, `-`, `@` in exports |
| TD Items declared | ✓ | TD-K-1 (in-process webhook delivery), TD-L+-1 (browser-only persistence) |

**Anti-bureaucracy test:** Every task spec is shorter than its expected code output. ✓

---

## Execution Sequence

```
EPIC K (Signals)
  K-1-T1  migrations (webhook, webhook_delivery)
  K-1-T2  webhook CRUD endpoints
  K-1-T3  delivery engine
  K-1-T4  Settings UI + test-fire
  K-2-T1  Slack Block Kit payload
  K-3-T1  scheduled_scan table + CRUD
  K-3-T2  scan scheduler background task
  K-4-T1  share token + public endpoint
  K-4-T2  ShareDiffPage component
  K-5-T1  digest scheduler + SMTP
  K-6-T1  GitHub status check on ack
  K-SMOKE  integration smoke suite
  ──── EPIC K retro ────

EPIC L (CSV Runner MVP)
  L-1-T1  csvParser utility + tests
  L-1-T2  variableExtractor utility + tests
  L-1-T3  CSV Runner tab in PlaygroundPage
  L-2-T1  variableResolver + secret masker
  L-2-T2  PreviewTable component
  L-3-T1  useRunEngine hook (execution + cancellation)
  L-3-T2  progress UI + results table
  L-4-T1  CSV export + formula injection protection
  L-SMOKE  browser smoke suite
  ──── EPIC L retro ────

EPIC L+ (expand if L retro confirms value)
  [Detailed after retro]
```

---

## Technical Debt Register

| ID | Component | Problem | Principal | Interest | Servicing Decision |
|----|-----------|---------|-----------|----------|-------------------|
| TD-K-1 | Webhook delivery engine | In-process Tokio task has no persistence across restarts; queued deliveries are lost on crash | Delivery reliability at >100 webhooks/min | Restart = lost deliveries; requires in-flight recovery | Replace with DB-backed queue (origin: ADR-K-1). Review at 10k deliveries/day |
| TD-L-1 | CSV Runner | Browser-side execution means runs are not auditable, not retryable across sessions, and fail on tab close | Full run history and audit trail | Users will request retry-from-failure and audit within weeks of MVP | Addressed by EPIC L+ (origin: Assumption A-4) |

# EPIC M — Full-Codebase Review Remediation

> **Objective:** Remediate the defects and security findings from the 2026-07-02 full-codebase expert review.
> **Mode:** HARDENING (no new features; REFACTORING/PREPARATORY/FEATURE-as-fix only)
> **Tracer Bullet?:** NO — builds on the existing end-to-end path (EPICs A–L complete)
> **Source:** Six parallel deep reviews (radar-api, radar-cli/core, radar-scanner, radar-ui, radar-desktop, cross-cutting CI/migrations)
> **Definition of Done (EPIC additions to Global DoD):**
> - [ ] Every P0 story has a failing regression test committed before its fix (Red→Green)
> - [ ] `cargo test` + `pnpm --recursive test` green; `cargo clippy -- -D warnings` clean
> - [ ] No secret, token, or DB credential reachable in logs or cross-origin
>
> **SLO additions:**
> - `radar-api – cross-org 403 rate = 100% over all authz integration tests`
> - `radar-api – migration apply success on PostgreSQL = 100% from clean DB`
> - `drift check – p99 wall-clock < 45s with an unresponsive upstream (timeout enforced)`

---

## Domain Glossary (terms used in this EPIC)

Uses existing glossary terms only: Producer, Consumer, Blast Radius, Breaking Change, Evidence, Fail Mode. No new terms introduced.

---

## Priority tiers

- **P0** — security-critical or breaks the core promise (drift detection / tenant isolation / Postgres deploy). Fix first.
- **P1** — correctness, robustness, and hardening that affects real users but is not exploitable-critical.
- **P2** — data quality, idempotency, UX, and cleanup.

---

## P0 — Critical

### Story M-1 · Catalog Sync Secret Exfiltration + SSRF

> **Persona:** Security engineer validating outbound-fetch safety
> **Value:** So that no caller can coerce the server into reading arbitrary environment variables or fetching internal/attacker URLs
> **Priority:** P0 (critical — credential theft + SSRF primitive)
> **Size:** M
> **Dependencies:** none
> **DoR status:** READY

**Finding:** `radar-api/src/catalog.rs:115-146` — `POST /v1/catalog-sources` reads any client-named env var (`token_env`) and sends it as a Bearer token to a client-supplied URL, with no SSRF guard and redirects enabled.

**Acceptance Criteria**
```gherkin
Given a catalog source with token_env = "RADAR_JWT_SECRET"
When sync runs
Then the request is rejected because token_env is not on the allowlist prefix

Given a catalog source url that resolves to a private/loopback/link-local address
When sync runs
Then the fetch is blocked by is_ssrf_blocked before any connection

Given the catalog sync HTTP client
Then it is built with .redirect(Policy::none()) and an explicit timeout
```

**Tasks**
| ID | Hat | Goal | Tier | Budget |
|---|---|---|---|---|
| M-1-T1 | FEATURE | Failing tests: token_env off-allowlist rejected; SSRF-target url blocked; redirects disabled | Sonnet | ≤2 500 |
| M-1-T2 | FEATURE | Allowlist `token_env` to a fixed prefix (e.g. `RADAR_CATALOG_TOKEN_*`); apply `is_ssrf_blocked` + `Policy::none()` + timeout to the catalog client | Sonnet | ≤2 500 |

---

### Story M-2 · Desktop Sidecar Auth + CORS Lockdown

> **Persona:** Desktop user with a browser open alongside API Radar
> **Value:** So that no website the user visits can read or mutate the local drift database or spend AI credits via the loopback sidecar
> **Priority:** P0 (critical — drive-by-localhost / DNS-rebinding)
> **Size:** M
> **Dependencies:** none
> **DoR status:** READY

**Finding:** Sidecar runs `CorsLayer::permissive()` with no auth (`radar-api/src/lib.rs:398-416`); desktop spawns it without `CORS_ALLOWED_ORIGINS` or a token (`radar-desktop/electron/main/index.ts`).

**Acceptance Criteria**
```gherkin
Given the sidecar started in desktop mode
Then a per-session bearer token is generated and required on all /v1 routes
And CORS is restricted to the desktop renderer origin (not permissive)

Given a cross-origin request from an arbitrary web origin without the token
When it calls any /v1 endpoint
Then it is rejected (401/403 or CORS-blocked)
```

**Tasks**
| ID | Hat | Goal | Tier | Budget |
|---|---|---|---|---|
| M-2-T1 | FEATURE | radar-api: when `RADAR_REQUIRE_AUTH`/desktop token set, require bearer on /v1; deny-by-default CORS when auth on | Sonnet | ≤3 000 |
| M-2-T2 | FEATURE | radar-desktop: generate a per-session token, pass via env to sidecar + IPC to renderer; apiClient attaches it | Sonnet | ≤3 000 |

---

### Story M-3 · Playground Iframe XSS + Token Exposure

> **Persona:** Team member using a shared sandbox environment
> **Value:** So that a malicious sandbox-env value cannot execute script in another user's browser or exfiltrate a bearer token
> **Priority:** P0 (critical — stored XSS)
> **Size:** S
> **Dependencies:** none
> **DoR status:** READY

**Finding:** `radar-ui/src/pages/PlaygroundPage.tsx:70-92` interpolates shared env values + `specUrl` raw into iframe `srcDoc` alongside `bearer_token`.

**Acceptance Criteria**
```gherkin
Given a sandbox env whose name/base_url contains "'><script>…"
When the playground iframe renders
Then the payload is HTML/attribute-escaped and does not execute
```

**Tasks**
| ID | Hat | Goal | Tier | Budget |
|---|---|---|---|---|
| M-3-T1 | FEATURE | Failing test on the escaper util with an XSS payload | Sonnet | ≤1 500 |
| M-3-T2 | FEATURE | Escape all interpolated values (attribute + JSON-in-HTML) or pass config via postMessage | Sonnet | ≤2 000 |

---

### Story M-4 · Diff Engine Direction-Awareness

> **Persona:** Producer running `drift check` in CI
> **Value:** So that request-body and parameter required/optional flips and content-type drops are detected as Breaking Changes instead of passing green
> **Priority:** P0 (breaks core promise)
> **Size:** L
> **Dependencies:** none
> **DoR status:** READY

**Findings:** `radar-core/src/diff.rs` — B1 request-body optional→required emitted Safe (694-719); B2 `requestBody.required` false→true unreported (386-437); B3 param optional→required unreported (283-302); B4 JSON content-type drop skipped (508-511); B5 path-level params ignored (210-232); B7 path-template rename false positive (150-152).

**Acceptance Criteria**
```gherkin
Given a request body field changes optional→required
Then a Breaking Change is emitted (not Safe) and exit code is nonzero under default policy

Given a query parameter changes optional→required
Then a Breaking Change is emitted

Given requestBody.required flips false→true
Then a Breaking Change is emitted

Given a response drops its application/json content entirely
Then a Breaking Change is emitted

Given a path template variable is renamed (/users/{id} → /users/{userId})
Then no OperationRemoved/OperationAdded false positive is emitted
```

**Tasks**
| ID | Hat | Goal | Tier | Budget |
|---|---|---|---|---|
| M-4-T1 | FEATURE | Failing tests for B1–B4 (direction-aware required + content-type drop) | Sonnet | ≤3 000 |
| M-4-T2 | FEATURE | Make required/severity direction-aware by prefix (request_body vs response); compare requestBody.required and parameter required | Sonnet | ≤3 000 |
| M-4-T3 | FEATURE | Detect JSON content-type removal; fold path-level parameters into resolved set (B5) | Sonnet | ≤2 500 |
| M-4-T4 | REFACTORING | Normalize path templates so variable renames don't produce phantom operation add/remove (B7) | Sonnet | ≤2 000 |

---

### Story M-5 · Proto Parser Error Handling

> **Persona:** Producer diffing protobuf contracts
> **Value:** So that malformed or wrong-format proto input fails loudly instead of silently reporting "no changes"
> **Priority:** P0 (silent pass on rewritten contract)
> **Size:** S
> **Dependencies:** none
> **DoR status:** READY

**Finding:** `radar-core/src/proto.rs:42-47` never returns `Err`; also drops `oneof`/`map<>` fields (174-219, 284-311).

**Acceptance Criteria**
```gherkin
Given a non-proto or malformed input passed to parse_proto
Then Err is returned (not an empty schema)

Given a oneof member or map<> field is removed
Then the change is detected
```

**Tasks**
| ID | Hat | Goal | Tier | Budget |
|---|---|---|---|---|
| M-5-T1 | FEATURE | Failing tests: garbage input → Err; oneof/map removal detected | Sonnet | ≤2 000 |
| M-5-T2 | FEATURE | Return Err on unparseable input (no `message`/`service`); parse oneof + map fields | Sonnet | ≤2 500 |

---

### Story M-6 · Scheduled Scan Baseline Off-By-One

> **Persona:** Platform engineer relying on scheduled scans to catch drift
> **Value:** So that a scheduled scan diffs against the immediately previous spec, not an empty or two-generations-old spec
> **Priority:** P0 (breaks core promise)
> **Size:** M
> **Dependencies:** none
> **DoR status:** READY

**Finding:** `radar-api/src/scans.rs:338-347` fetches base with `OFFSET 1` before storing the new spec, so the real previous spec is skipped; three rows share `captured_at` making ordering nondeterministic.

**Acceptance Criteria**
```gherkin
Given a service scanned twice with a Breaking Change on the 2nd scan
When the 2nd scan executes
Then the diff is computed against the 1st scan's spec and the change is reported
```

**Tasks**
| ID | Hat | Goal | Tier | Budget |
|---|---|---|---|---|
| M-6-T1 | FEATURE | Failing test: 2-scan sequence reports the change | Sonnet | ≤2 500 |
| M-6-T2 | FEATURE | Capture base spec before storing new spec (or `OFFSET 0`); tie-break ordering by a monotonic id | Sonnet | ≤2 000 |

---

### Story M-7 · Migration PostgreSQL Portability

> **Persona:** Operator deploying to PostgreSQL
> **Value:** So that a clean `sqlx migrate run` succeeds on PostgreSQL and list endpoints do not 500
> **Priority:** P0 (blocks production deploy)
> **Size:** M
> **Dependencies:** none
> **DoR status:** READY

**Findings:** `strftime()` defaults in migrations 014–017 (SQLite-only); `ORDER BY rowid` in `webhooks.rs:561`; negative/unclamped LIMIT across list handlers.

**Acceptance Criteria**
```gherkin
Given a clean PostgreSQL database
When sqlx migrate run executes all migrations
Then it completes without error

Given GET /v1/webhooks/:id/deliveries on PostgreSQL
Then it returns 200 (ordered by a real column, not rowid)

Given ?limit=-1 on any list endpoint
Then it is clamped to a safe range on both SQLite and PostgreSQL
```

**Tasks**
| ID | Hat | Goal | Tier | Budget |
|---|---|---|---|---|
| M-7-T1 | FEATURE | New migration: set `created_at` defaults application-side (bind in code) or portable default; add `webhook_delivery.created_at` + index | Sonnet | ≤2 500 |
| M-7-T2 | FEATURE | Replace `ORDER BY rowid` with `created_at`; add a shared `clamp_pagination()` and apply to all list handlers | Sonnet | ≤3 000 |

---

## P1 — Correctness, robustness, hardening

### Story M-8 · Org Isolation Sweep

> **Persona:** Security engineer validating multi-tenant isolation
> **Value:** So that no org can read or mutate another org's release notes, settings, acknowledgements, scans, subscriptions, or AI artifacts
> **Priority:** P1 (re-opens E-2, which was believed complete)
> **Size:** L
> **Dependencies:** none
> **DoR status:** READY

**Finding:** ~1/3 of handlers skip org checks (`release_notes.rs`, `settings.rs`, `acknowledgements.rs`, `scans.rs`, `consumers.rs` subscription, `ai_tests.rs`, `summary.rs`, `notifications.rs`, `playground.rs`); `assert_org_access` skips on empty side (`auth.rs:456-468`); audit read/write org mismatch (`audit.rs:129`).

**Acceptance Criteria**
```gherkin
Given org A's token
When any of the enumerated endpoints is called with an org B resource id
Then 403 is returned — not 200, not 404
```

**Tasks**
| ID | Hat | Goal | Tier | Budget |
|---|---|---|---|---|
| M-8-T1 | FEATURE | Shared `require_org_owned(pool, table, id, org)` helper + failing cross-org test matrix over all enumerated endpoints | Sonnet | ≤3 000 |
| M-8-T2 | FEATURE | Apply the helper to release notes, settings, acknowledgements, scans, subscriptions, ai_tests, summary, notifications, playground | Sonnet | ≤3 500 |
| M-8-T3 | FEATURE | Fix audit event org write/read consistency; scope `PUT /v1/settings` to caller org | Sonnet | ≤2 500 |

---

### Story M-9 · API Infra Security Hardening

> **Persona:** Operator running the web deployment
> **Value:** So that the rate limiter cannot be bypassed, DB credentials never hit logs, and secret comparisons are constant-time
> **Priority:** P1
> **Size:** M
> **Dependencies:** none
> **DoR status:** READY

**Findings:** rate limiter keys on unvalidated bearer + trusts XFF (`lib.rs:117-139`); DB URL logged (`main.rs:49`); `==` secret compares (`auth.rs:441`, `lib.rs:543`, `scalar_update.rs:131`).

**Acceptance Criteria**
```gherkin
Given anonymous requests with random bearer tokens
Then they share a rate-limit bucket (keyed on peer/validated identity), not a fresh one each

Given the service starts with a postgres:// URL
Then the logged connection string has userinfo redacted

Given a service/metrics token comparison
Then it uses a constant-time compare
```

**Tasks**
| ID | Hat | Goal | Tier | Budget |
|---|---|---|---|---|
| M-9-T1 | FEATURE | Redact DB URL userinfo before logging; constant-time compares (`subtle`) | Sonnet | ≤2 000 |
| M-9-T2 | FEATURE | Re-key rate limiter on validated identity/peer addr; only trust XFF behind a configured proxy | Sonnet | ≤2 500 |

---

### Story M-10 · Postman Parser Accuracy

> **Persona:** Consumer team importing a Postman collection as Evidence
> **Value:** So that real collections (nested folders, `:var` params, query strings) produce correct, matchable Evidence
> **Priority:** P1
> **Size:** M
> **Dependencies:** none
> **DoR status:** READY

**Findings:** `radar-scanner/src/lib.rs` — folder recursion dropped (411-427); `:userId` not normalized (453-518); `pm.response.*` false field paths (622-639); query/fragment/trailing-slash not stripped; raw-less url object ignored; BOM rejected.

**Acceptance Criteria**
```gherkin
Given a collection with nested folders
Then requests inside folders are extracted

Given a Postman URL /users/:userId
Then the operation normalizes to /users/{userId}

Given a test script line "pm.response.json()"
Then no field path named "json"/"to" is emitted
```

**Tasks**
| ID | Hat | Goal | Tier | Budget |
|---|---|---|---|---|
| M-10-T1 | FEATURE | Failing tests: nested folders, `:var` params, query strings, `pm.response` false-positive, BOM | Sonnet | ≤2 500 |
| M-10-T2 | FEATURE | Recurse folder items; normalize `:var`→`{var}`; strip query/fragment/trailing slash; word-boundary field extraction; strip BOM; honor host/path when raw absent | Sonnet | ≤3 000 |

---

### Story M-11 · TypeScript/TSX Scanner Accuracy

> **Persona:** Consumer team with a React/TS codebase
> **Value:** So that `.tsx` files parse correctly and field access is attributed to the right operation
> **Priority:** P1
> **Size:** M
> **Dependencies:** none
> **DoR status:** READY

**Findings:** `.tsx` parsed with non-JSX grammar (`lib.rs:29-35`); file-wide first-operation stamping (298-311); plain JS unsupported (20-27).

**Acceptance Criteria**
```gherkin
Given a .tsx file with JSX and member access
Then it parses without ERROR nodes and member access is captured

Given a file with two API calls
Then field access is attributed to the enclosing call/function, not the first call for the whole file

Given a plain .js/.jsx/.mjs file
Then it is scanned (TS grammar)
```

**Tasks**
| ID | Hat | Goal | Tier | Budget |
|---|---|---|---|---|
| M-11-T1 | FEATURE | Failing tests: TSX JSX file, two-call attribution, .js support | Sonnet | ≤2 500 |
| M-11-T2 | FEATURE | Use TSX grammar for `.tsx`; add JS extensions; scope operation attribution to enclosing function | Sonnet | ≤3 000 |

---

### Story M-12 · CLI Policy + Network Robustness

> **Persona:** Producer with `fail_mode: open` and CI running against a flaky API
> **Value:** So that a valid label override is honored when the API is down, and `drift check` never hangs a CI job
> **Priority:** P1
> **Size:** M
> **Dependencies:** none
> **DoR status:** READY

**Findings:** `policy.rs:162-166` drops label override + `block_on` in fail-open when api_error; no reqwest timeouts (`api_client.rs`, `github.rs`); GitHub comment listing unpaginated (`github.rs:383`).

**Acceptance Criteria**
```gherkin
Given fail_mode: open, api_error, a Breaking Change, and the drift-ack label present
Then decide() returns pass (override honored)

Given an upstream that accepts TCP but never responds
Then drift check fails within a bounded timeout, not indefinitely

Given a PR with >100 comments
Then the marker comment is found via pagination (no duplicate posted)
```

**Tasks**
| ID | Hat | Goal | Tier | Budget |
|---|---|---|---|---|
| M-12-T1 | FEATURE | Failing tests: fail-open + api_error + override; timeout path; comment pagination | Sonnet | ≤2 500 |
| M-12-T2 | FEATURE | Honor label override + `block_on` in fail-open; add reqwest timeouts to all clients; paginate comment listing | Sonnet | ≤3 000 |

---

### Story M-13 · Desktop Sidecar Lifecycle

> **Persona:** Desktop user launching the app more than once / after a crash
> **Value:** So that a second launch doesn't kill the first instance's sidecar, quit doesn't leak zombies, and a startup failure is recoverable
> **Priority:** P1
> **Size:** M
> **Dependencies:** M-2 (touches same file)
> **DoR status:** READY

**Findings:** no single-instance lock (B1); no tree-kill on quit (B2); PID reuse risk (B3); unclosable splash on failure (B4); no crash detection (B5).

**Acceptance Criteria**
```gherkin
Given an instance is already running
When a second launch occurs
Then the existing window is focused and the running sidecar is not killed

Given the app quits (incl. cargo-run dev fallback)
Then the sidecar process tree is fully terminated

Given the sidecar fails to start
Then the user gets a dialog with retry/quit, not a frozen splash
```

**Tasks**
| ID | Hat | Goal | Tier | Budget |
|---|---|---|---|---|
| M-13-T1 | FEATURE | `requestSingleInstanceLock()` + focus-existing on second-instance | Sonnet | ≤2 000 |
| M-13-T2 | FEATURE | Tree-kill sidecar on quit (`taskkill /T` / process-group); verify image name before killing a recorded PID | Sonnet | ≤2 500 |
| M-13-T3 | FEATURE | Recoverable startup-failure dialog; post-startup crash detection with notify/restart | Sonnet | ≤2 500 |

---

### Story M-14 · UI Production Basename Fixes

> **Persona:** External recipient opening a public share link on the web deployment
> **Value:** So that share links and audit links work under the `/app` basename
> **Priority:** P1
> **Size:** S
> **Dependencies:** none
> **DoR status:** READY

**Findings:** `/share/` path check misses `/app/share/…` (`App.tsx:220-227`); audit raw `<a href>` 404s under `/app` (`AuditPage.tsx:179,254`).

**Acceptance Criteria**
```gherkin
Given the app served under base /app
When an unauthenticated user opens /app/share/<token>
Then ShareDiffPage renders without auth redirect and without sidebar chrome

Given the audit page under /app
When a diff link is clicked
Then it routes within the SPA (uses <Link>, respects basename)
```

**Tasks**
| ID | Hat | Goal | Tier | Budget |
|---|---|---|---|---|
| M-14-T1 | FEATURE | Basename-aware share-route detection; replace raw anchors with `<Link>` | Sonnet | ≤2 000 |

---

### Story M-15 · UI Release-Notes Async Polling

> **Persona:** User generating release notes from a diff
> **Value:** So that generated release notes actually appear (API is async: 201 pending + poll)
> **Priority:** P1
> **Size:** S
> **Dependencies:** none
> **DoR status:** READY

**Finding:** `DiffDetailPage.tsx:168-180` expects `{content}` but API returns `{generation_status:"pending"}` and exposes a poll endpoint the UI never calls.

**Acceptance Criteria**
```gherkin
Given the user clicks Generate Release Notes
When the API returns pending
Then the UI polls generate-status until complete and renders the content
And shows an error state on generation failure
```

**Tasks**
| ID | Hat | Goal | Tier | Budget |
|---|---|---|---|---|
| M-15-T1 | FEATURE | Poll `generate-status` after generate; render result/error; cleanup on unmount | Sonnet | ≤2 500 |

---

### Story M-16 · CI / Release Pipeline Fixes

> **Persona:** Maintainer pushing to `main` for the first time
> **Value:** So that CI runs, the desktop release produces a working installer, and E2E targets the right port
> **Priority:** P1
> **Size:** M
> **Dependencies:** none
> **DoR status:** READY

**Findings:** setup-node `cache: pnpm` before pnpm install (ci.yml, release.yml); release runs `electron-builder` directly, skipping `electron-vite build` + `build.config.json`; Playwright targets 5173 vs Vite 6173; desktop never linted/tested.

**Acceptance Criteria**
```gherkin
Given a push to main
Then the node job installs pnpm before setup-node's cache step

Given the desktop release job
Then it runs electron-vite build and electron-builder with build.config.json (sidecar bundled)

Given the E2E job
Then Playwright targets the port Vite actually serves
```

**Tasks**
| ID | Hat | Goal | Tier | Budget |
|---|---|---|---|---|
| M-16-T1 | FEATURE | Reorder pnpm/setup-node; fix release build invocation; align Playwright/Vite port; add desktop lint/typecheck to CI | Sonnet | ≤3 000 |

---

## P2 — Data quality, idempotency, UX, cleanup

### Story M-17 · API Idempotency + Data Quality

> **Persona:** Platform engineer viewing blast radius / ingesting batches
> **Value:** So that repeated GETs don't inflate Evidence, batch ingest is atomic, and share tokens aren't guessable
> **Priority:** P2
> **Size:** M
> **Dependencies:** none
> **DoR status:** READY

**Findings:** blast-radius GET inserts duplicate evidence (`diffs.rs:705-744`); non-atomic batch ingest (`ingestion.rs:76-97`); deterministic share tokens (`diffs.rs:359`); `mask_token` panic (`playground.rs:24-30`); Slack relative URL (`webhooks.rs:376`); concurrent-diff 500 (`diffs.rs:242-277`).

**Tasks**
| ID | Hat | Goal | Tier | Budget |
|---|---|---|---|---|
| M-17-T1 | FEATURE | Make blast-radius evidence write idempotent (deterministic id) or move off the GET path | Sonnet | ≤2 500 |
| M-17-T2 | FEATURE | Wrap batch ingest in a transaction; return 4xx on FK/user error | Sonnet | ≤2 500 |
| M-17-T3 | FEATURE | Random, revocable share tokens; fix `mask_token` char-boundary; absolute Slack URL; treat unique-violation as cached-200 | Sonnet | ≤3 000 |

### Story M-18 · UI Robustness Sweep

> **Persona:** Any dashboard user during a transient API failure
> **Value:** So that list errors don't masquerade as empty states, forms don't fire unintended saves, and fetches are abortable
> **Priority:** P2
> **Size:** M
> **Dependencies:** none
> **DoR status:** READY

**Findings:** Settings implicit submit + nested forms (`SettingsPage.tsx`); silent error→empty-state across pages; no AbortController; diffs list unpaginated (H5); service→diffs filter no-op (H6); timezone bucketing mismatch (M6); AIR token violations (hardcoded hex).

**Tasks**
| ID | Hat | Goal | Tier | Budget |
|---|---|---|---|---|
| M-18-T1 | REFACTORING | Shared abortable `useFetch` with uniform error/empty states | Sonnet | ≤3 000 |
| M-18-T2 | FEATURE | Fix Settings button `type`/nesting; wire diffs pagination + service filter; align timezone; replace hardcoded hex with AIR tokens | Sonnet | ≤3 000 |

### Story M-19 · Workspace + Docs Hygiene

> **Persona:** New contributor
> **Value:** So that docs/ports match reality and untracked artifacts are committed
> **Priority:** P2
> **Size:** S
> **Dependencies:** none
> **DoR status:** READY

**Findings:** openapi.yaml missing 2 routes; README port/Node contradictions; `BadRequest` 422 vs documented 400; untracked `AGENTS.md`/`cliff-notes.md`/icon; Vite 5/6 skew; unused `tokio-util`.

**Tasks**
| ID | Hat | Goal | Tier | Budget |
|---|---|---|---|---|
| M-19-T1 | REFACTORING | Sync openapi.yaml + README ports/Node; align error status; commit intended untracked files; dedupe deps | Sonnet | ≤2 500 |

---

## Execution order & parallelism

Sequential cargo builds only (disk constraint). Parallelism is across **ecosystems**, not within the Rust build:

1. **Wave 1 (P0):** M-1, M-4, M-5, M-6, M-7 (Rust, sequential) ‖ M-3 (UI) ‖ M-2 (Rust+desktop pair)
2. **Wave 2 (P1):** M-8, M-9 (Rust) ‖ M-10, M-11 (Rust scanner) ‖ M-12 (Rust CLI) ‖ M-13 (desktop) ‖ M-14, M-15 (UI) ‖ M-16 (CI yaml)
3. **Wave 3 (P2):** M-17 (Rust) ‖ M-18 (UI) ‖ M-19 (docs)

Rust stories run one-at-a-time through `cargo test`; non-Rust stories (UI, desktop, CI, docs) can proceed concurrently.

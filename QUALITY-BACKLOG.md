# EPIC N — Post-Remediation Quality & Completeness

> **Objective:** Close the correctness, functionality, and structural gaps found in the 2026-07-02 full-codebase functionality+quality review (the follow-up to EPIC M).
> **Mode:** Mixed — HARDENING for the correctness bugs, DELIVERY/FEATURE for diff-engine + scanner completeness, REFACTORING for the architecture debt. Declared per story.
> **Source:** Four parallel deep reviews (radar-api; radar-core+cli; scanner+ui+desktop; architecture+tests+CI).
> **Relationship to EPIC M:** EPIC M fixed the security/correctness review findings and got CI green. This EPIC covers what that review deferred (M-20 PostgreSQL, M-21 settings — folded in here as N-1/N-2 tier work) plus the *new* findings surfaced once the codebase was re-reviewed for functionality and quality.

**EPIC Definition of Done (additions to Global DoD):**
- [ ] Every P0 story has a failing regression test committed before the fix (the P0 bugs are all currently untested).
- [ ] `cargo test` + `pnpm --recursive test` green; `cargo clippy -- -D warnings` clean; CI green.
- [ ] "No changes detected" is trustworthy for the change classes covered by N-4…N-8.

**SLO additions:**
- `radar-core – radar check never panics on any well-formed spec (incl. recursive $ref) = 100%`
- `radar-api – diff persistence is atomic (no partial diffs) = 100%`
- `radar-api – per-service sampling keep-rate error < 5% of configured rate`

---

## Domain Glossary

Uses existing terms only: Producer, Consumer, Blast Radius, Breaking Change, Evidence, Fail Mode. No new terms.

## Priority tiers

- **P0** — silent correctness failures or crashes (the tool gives a wrong/absent answer, or dies). Fix first.
- **P1** — functionality gaps in the core promise, real robustness/security bugs, and the strategic Postgres decision.
- **P2** — structural debt, test/CI/hygiene, and lower-severity polish.

---

# P0 — Silent correctness failures & crashes

### Story N-1 · Diff engine recursion guard
> **Persona:** Producer running `drift check` on a spec with recursive schemas
> **Value:** So that a self-referential `$ref` (a tree, a comment thread, `User.manager → User`) does not crash the tool
> **Priority:** P0 (crash) · **Size:** S · **Hat:** FEATURE (bug fix) · **DoR:** READY

**Finding:** `radar-core/src/diff.rs` `diff_schema_properties` recurses on nested object properties with no cycle guard or depth cap → stack overflow when a component schema references itself (present in both specs). Recursive schemas are common.

**AC**
```gherkin
Given a spec whose User schema has a property of type $ref '#/components/schemas/User'
When drift check runs against a copy (or a modified copy)
Then it completes without stack overflow and reports the intended change (or none)
```
**Tasks**
| ID | Hat | Goal | Tier | Budget |
|---|---|---|---|---|
| N-1-T1 | FEATURE | Failing test: recursive `$ref` schema in both base+head → no panic | Sonnet | ≤1 500 |
| N-1-T2 | FEATURE | Track visited `$ref` targets (or a max depth) per path in `diff_schema_properties`; stop recursion on revisit | Sonnet | ≤2 000 |

---

### Story N-2 · Fix per-service sampling math
> **Persona:** Platform engineer configuring ingestion sampling to control cost
> **Value:** So that a configured `sample_rate` actually samples at that rate
> **Priority:** P0 (feature numerically broken, untested) · **Size:** S · **Hat:** FEATURE (bug fix) · **DoR:** READY

**Finding:** `radar-api/src/utils.rs` `sample_keep` divides `subsec_nanos()` (max ~1e9) by `u32::MAX` (~4.29e9), so the value never exceeds ~0.233; any `sample_rate ≥ 0.24` keeps 100% and `0.1` keeps ~43%. Zero tests exist for it.

**AC**
```gherkin
Given sample_rate = 0.1
When 10,000 events are evaluated
Then roughly 10% (±a few %) are kept — not 43%
Given sample_rate = 0.5 then ~50% kept; rate = 1.0 keeps all; rate = 0.0 keeps none
```
**Tasks**
| ID | Hat | Goal | Tier | Budget |
|---|---|---|---|---|
| N-2-T1 | FEATURE | Failing statistical test over N samples for several rates | Sonnet | ≤1 500 |
| N-2-T2 | FEATURE | Divide by `1_000_000_000` (or use a proper RNG in [0,1)); keep it deterministic-testable | Sonnet | ≤1 000 |

---

### Story N-3 · Atomic diff persistence
> **Persona:** Platform engineer relying on stored diffs for blast radius and policy
> **Value:** So that a diff is never persisted with a partial set of changes that the dedup cache then makes permanent
> **Priority:** P0 (silent data corruption) · **Size:** M · **Hat:** REFACTORING+FEATURE · **DoR:** READY

**Finding:** `radar-api/src/diffs.rs` (and the sibling copies in `compare_specs`, `run_batch_item`, `scans.rs::create_scan_diff`) insert the diff row + change rows in a loop with no transaction. A mid-loop failure leaves a truncated diff; every retry then hits the `idx_diff_transition` dedup path and returns `cached: true`, so the truncation is permanent — corrupting blast radius, policy decisions, and release notes.

**AC**
```gherkin
Given a change insert fails partway through creating a diff
Then the whole diff creation rolls back (no diff row, no partial changes)
And a retry re-creates the complete diff
```
**Tasks**
| ID | Hat | Goal | Tier | Budget |
|---|---|---|---|---|
| N-3-T1 | REFACTORING | Extract the diff+changes persistence into one `persist_diff(tx, …)` helper (removes the 4× duplication) | Sonnet | ≤3 000 |
| N-3-T2 | FEATURE | Wrap it in a single sqlx transaction; failing test asserts no partial diff on mid-loop error | Sonnet | ≤2 500 |

---

# P1 — Core-promise functionality gaps

### Story N-4 · Diff array-of-object responses
> **Persona:** Producer whose list endpoints return arrays
> **Value:** So that removing a field from `GET /users → [User]` is detected as breaking
> **Priority:** P1 (silent miss on the most common shape) · **Size:** M · **Hat:** FEATURE · **DoR:** READY

**Finding:** `diff.rs` `diff_schema_properties` only recurses when both schemas are `Type::Object`; an array-of-objects response/request gets no item-level diffing and no top-level type comparison → "No changes detected" on a real breaking change.

**AC**
```gherkin
Given a 200 response schema of type array whose items lose a required field
Then a Breaking FieldRemoved is reported for the item field
Given the array item type changes (string→object) then a Breaking TypeChanged is reported
```
**Tasks**
| ID | Hat | Goal | Tier | Budget |
|---|---|---|---|---|
| N-4-T1 | FEATURE | Failing tests: array-item field removal + array-item type change | Sonnet | ≤2 000 |
| N-4-T2 | FEATURE | Recurse into `Type::Array` item schemas (resolve $ref) and compare | Sonnet | ≤2 500 |

---

### Story N-5 · Diff composed schemas (allOf / oneOf / anyOf)
> **Persona:** Producer using schema composition (allOf inheritance, oneOf unions)
> **Value:** So that changes inside composed schemas are not invisible
> **Priority:** P1 · **Size:** L · **Hat:** FEATURE · **DoR:** READY

**Finding:** `SchemaKind::OneOf/AllOf/AnyOf/Any` yield `None` from `type_label_from_kind` and no recursion; composed schemas (allOf is ubiquitous) are entirely undiffed.

**AC**
```gherkin
Given an allOf-composed response schema loses a field from one of its members
Then a Breaking change is reported
Given a oneOf variant is removed then a Breaking change is reported
```
**Tasks**
| ID | Hat | Goal | Tier | Budget |
|---|---|---|---|---|
| N-5-T1 | FEATURE | Failing tests for allOf member field removal + oneOf variant removal | Sonnet | ≤2 500 |
| N-5-T2 | FEATURE | Flatten `allOf` into a merged property set; diff `oneOf`/`anyOf` variants by index/position with add/remove detection | Sonnet | ≤3 500 |

---

### Story N-6 · Protobuf service / nested / enum-number coverage
> **Persona:** Producer diffing gRPC protobuf contracts
> **Value:** So that removing an RPC method, changing a nested field, or changing an enum value's number is detected
> **Priority:** P1 · **Size:** M · **Hat:** FEATURE · **DoR:** READY

**Finding:** `proto.rs` — `service { rpc … }` blocks are skipped by the top-level scanner (RPC removal/rename undetected — the headline proto break); nested message bodies are brace-skipped; enum values are diffed by name only (number change is wire-breaking but undetected); packages are ignored so same-named messages collide.

**AC**
```gherkin
Given an rpc method is removed from a service then a Breaking OperationRemoved is reported
Given an enum value's number changes then a Breaking change is reported
Given a field inside a nested message is removed then it is reported
```
**Tasks**
| ID | Hat | Goal | Tier | Budget |
|---|---|---|---|---|
| N-6-T1 | FEATURE | Failing tests: rpc removal, enum-number change, nested-message field removal | Sonnet | ≤2 500 |
| N-6-T2 | FEATURE | Parse `service`/`rpc`, recurse nested messages, diff enum numbers, key messages by package-qualified name | Sonnet | ≤3 000 |

---

### Story N-7 · GraphQL argument-type + input-object direction + extends
> **Persona:** Producer diffing GraphQL SDL
> **Value:** So that argument-type changes and required input-field additions are flagged breaking
> **Priority:** P1 · **Size:** M · **Hat:** FEATURE · **DoR:** READY

**Finding:** `graphql.rs` — `diff_args` only detects added/removed args, not an existing arg's type change (`String → String!`); the EPIC M direction-awareness never reached GraphQL, so adding a non-null field to an input type is reported Safe though it breaks callers; `extend type` blocks are dropped (false FieldRemoved).

**AC**
```gherkin
Given an argument's type changes to non-null then a Breaking change is reported
Given a non-null field is added to an input type then a Breaking change is reported
Given a field is defined via `extend type` then it is not reported as removed
```
**Tasks**
| ID | Hat | Goal | Tier | Budget |
|---|---|---|---|---|
| N-7-T1 | FEATURE | Failing tests for arg-type change, input-object non-null add, extend-type | Sonnet | ≤2 500 |
| N-7-T2 | FEATURE | Diff arg types; make field-required severity direction-aware (object vs input); merge `extend type` into the base type | Sonnet | ≤3 000 |

---

### Story N-8 · OpenAPI coverage breadth (constraints, security, headers, non-2xx, media types, param $ref)
> **Persona:** Producer relying on the diff for all contract dimensions
> **Value:** So that format/constraint/security/header/error-response drift is detected instead of silently ignored
> **Priority:** P1 · **Size:** L · **Hat:** FEATURE · **DoR:** READY

**Finding:** `diff.rs` does not detect: `format` (int32→int64, date→date-time), numeric/string constraints (`minimum`, `maxLength`, `pattern`), `additionalProperties`, security schemes/requirements, `servers`, response headers, non-2xx responses (and no `ResponseAdded`), non-`application/json` media types, and parameter type changes routed through a `$ref` (`param_type_label` returns None on refs). Also `'200'` vs `'2XX'` produce a false `ResponseRemoved`.

**AC** — each dimension above produces the correct change kind/severity with a test; documented if intentionally out of scope.
**Tasks**
| ID | Hat | Goal | Tier | Budget |
|---|---|---|---|---|
| N-8-T1 | FEATURE | Failing tests per dimension (batch) | Sonnet | ≤3 000 |
| N-8-T2 | FEATURE | Add constraint/format/additionalProperties diffing + resolve param `$ref` for type; fix 200-vs-2XX keying | Sonnet | ≤3 500 |
| N-8-T3 | FEATURE | Add security-scheme, server, response-header, non-2xx and ResponseAdded diffing | Sonnet | ≤3 500 |

---

## P1 — API correctness, robustness, security

### Story N-9 · Ingestion honesty (FK failures → 4xx, don't count as accepted)
> **Priority:** P1 · **Size:** S · **Hat:** FEATURE
> **Finding:** `ingestion.rs` OTLP/gateway inserts use `let _ =` and increment `accepted` even when the insert fails (e.g. FK violation) — client told data was stored when it was dropped; inconsistent with `/v1/usage/events` which maps FK errors to 4xx.
**AC:** a failed row does not increment `accepted`; a user-caused constraint violation returns 4xx; test with an unknown consumer_id.

### Story N-10 · SSRF DNS-rebinding + non-blocking DNS
> **Priority:** P1 · **Size:** M · **Hat:** FEATURE
> **Finding:** `utils.rs is_ssrf_blocked` resolves DNS at check time; reqwest resolves again at send time → rebinding bypass (webhooks/scans/csv). Also `to_socket_addrs()` is a blocking syscall on the async runtime, per webhook/scan create and per CSV row.
**AC:** resolve once and pin the connection to the vetted IP (or block private IPs at connect via a custom resolver); DNS resolution runs off the async runtime (`spawn_blocking`/async resolver); tests for a rebinding-style host and IPv4-mapped IPv6.

### Story N-11 · Per-org weekly digest
> **Priority:** P1 · **Size:** S · **Hat:** FEATURE
> **Finding:** `notifications.rs` scheduled digest aggregates all orgs (`org_id=""`) into one email to a global recipient list — cross-org disclosure.
**AC:** the digest is computed and sent per org to that org's recipients; no cross-org rows in an org's email.

### Story N-12 · Share-token intent + shared-view severity parity
> **Priority:** P1 · **Size:** M · **Hat:** FEATURE
> **Finding:** `GET /v1/diffs/:id` mints a public `/share/:token` as a side effect of every first view; the shared view bypasses evolution-rule severity overrides applied in the org view (inconsistent severities).
**AC:** share tokens are created only via an explicit share action (not on read); the shared view applies the same evolution-rule severities as the org view.

### Story N-13 · Uniform pagination clamping
> **Priority:** P1 · **Size:** S · **Hat:** REFACTORING
> **Finding:** `clamp_pagination` (utils.rs) exists but isn't applied in `audit.rs`, `csv_runner.rs`, `decisions.rs`, `acknowledgements.rs` — `limit=-1` dumps the table on SQLite / errors on Postgres.
**AC:** every list handler routes limit/offset through `clamp_pagination`; a negative-limit test per endpoint.

### Story N-14 · Scheduled-scan serialization
> **Priority:** P1 · **Size:** M · **Hat:** FEATURE
> **Finding:** `scans.rs` scheduler has no cross-instance lock and can double-fire within one instance if `execute_scan` stalls >60s before its first UPDATE; two replicas double-run every scan.
**AC:** a scan claims a lease (set `last_run_at`/status before work, `FOR UPDATE SKIP LOCKED` or advisory lock on the multi-instance path) so it runs at most once per interval; also fixes `fetch_previous_spec` to use the scan's own prior spec, not any origin's newest.

### Story N-15 · CLI remaining timeouts + panic guard
> **Priority:** P1 · **Size:** S · **Hat:** FEATURE
> **Finding:** `explain.rs`, `jira.rs`, `postman.rs`, `register.rs`, `main.rs:687` build `Client::new()` with no timeout (M-12 covered only api_client/github/ai_provider); `explain.rs:134` byte-slices `&diff.id[..8]` and panics if shorter.
**AC:** all reqwest clients have connect+read timeouts (shared builder); id-slice uses a char-safe/length-checked truncation; unit test for the short-id case.

---

## P1 — Scanner precision & reach

### Story N-16 · Scanner evidence precision
> **Persona:** Consumer team trusting Blast Radius
> **Value:** So that Blast Radius is built from real API field access, not every `console.log`
> **Priority:** P1 · **Size:** M · **Hat:** FEATURE · **DoR:** READY
> **Finding:** `radar-scanner/src/lib.rs` + `radar-cli/src/main.rs` post *every* member-expression leaf (including `console.log`, `JSON.parse`, and the API method name itself) as a call-site field access → floods `impact_evidence`, inflates Blast Radius, devalues confidence.
**AC:** only field accesses on a value derived from an API call (or matching a known API object) are emitted as evidence; a test asserts `console.log`/`JSON.parse` produce no evidence; fixes the first-op scope pinning (finding: a function's second API call's fields are mis-attributed).

### Story N-17 · Scanner reach — direct HTTP clients + more languages
> **Priority:** P1 · **Size:** L · **Hat:** FEATURE
> **Finding:** S2 only fires on `obj.method()` where the receiver contains "api"/"client"; direct `fetch("/users/1")`, `axios.get(...)`, `requests.get(...)`, `http.Get(...)` and string-literal URL args are never inspected; Java/C#/Ruby consumers are invisible with no warning.
**AC:** recognize well-known HTTP clients and extract string-literal URL paths into operations; add Java/C#/Ruby grammars (or explicitly log unsupported languages found).

### Story N-18 · Scanner robustness
> **Priority:** P1 · **Size:** S · **Hat:** FEATURE
> **Finding:** `walk()` follows symlinks with no cycle guard or depth cap (symlink loop → stack overflow); files are slurped with no size cap (multi-MB vendored bundles).
**AC:** use `file_type()` (no symlink follow) or a visited-inode set + depth cap; skip files above a size threshold (~2 MB) with a logged count.

---

## P1 — Desktop hardening & functional break

### Story N-19 · Desktop navigation / window-open / permission / IPC-sender hardening
> **Priority:** P1 · **Size:** M · **Hat:** FEATURE
> **Finding:** No `will-navigate` or `setWindowOpenHandler` on any window (a popup escaping the playground iframe, or a compromised renderer, can open arbitrary remote content in a privileged window); `get-api-token` IPC doesn't validate `event.senderFrame`; no `setPermissionRequestHandler`.
**AC:** `will-navigate` pins navigation to the app origin; `setWindowOpenHandler` denies and routes external https to `shell.openExternal`; permission requests denied by default; `get-api-token` validates the sender origin.

### Story N-20 · Packaged CSP allows the Playground
> **Priority:** P1 (production-only functional break) · **Size:** S · **Hat:** FEATURE
> **Finding:** `radar-desktop/src/renderer/index.html` CSP `script-src 'self'` / `connect-src http://127.0.0.1:17380` — srcdoc iframes inherit the parent CSP, so the Scalar bundle (and default CDN spec URL) are blocked in the packaged build; dev mode hides it (Vite CSP-less HTML).
**AC:** the packaged app renders the Playground iframe (CSP allows the sidecar `scalar.js` and the Scalar demo/CDN sources, or the bundle is served locally); verified in a packaged build or an equivalent CSP unit check.

### Story N-21 · Desktop auto-update + crash reporting
> **Priority:** P1 · **Size:** M · **Hat:** FEATURE
> **Finding:** `electron-updater` is a declared-but-unwired dependency; `build.config.json` has no publish config; no `crashReporter` and no `render-process-gone`/`child-process-gone` handlers.
**AC:** either wire `autoUpdater.checkForUpdatesAndNotify()` with a publish feed or remove the dependency; log render/child-process-gone to the sidecar log; also reset `restartAttempted` after a successful auto-restart so each incident gets one retry, and validate the health check's response body/port.

---

## P1 — UI quality

### Story N-22 · Accessibility pass
> **Persona:** Keyboard / screen-reader user
> **Value:** So that the dashboard is operable without a mouse
> **Priority:** P1 (highest UI quality gap) · **Size:** L · **Hat:** FEATURE · **DoR:** READY
> **Finding:** Across ~9,600 LOC there is one each of `aria-label`/`role`/`tabIndex`/`onKeyDown`; clickable `<tr onClick>` rows are keyboard-unreachable; icon-only buttons rely on `title`.
**AC:** interactive rows are buttons/links or have `role`+`tabIndex`+key handlers; icon-only controls have `aria-label`; forms associate `label`/`id`; passes an axe smoke check on the main pages.
**Tasks:** T1 add an axe/RTL a11y test harness; T2 remediate rows, buttons, forms across pages.

### Story N-23 · Shared abortable fetch + honest error states
> **Priority:** P1 · **Size:** M · **Hat:** REFACTORING
> **Finding:** ~15 pages hand-roll `useEffect`+`useState`+`api.get` with inconsistent cancellation; `HomePage`/`SettingsPage` swallow errors with `.catch(()=>{})` so failure looks identical to empty; `DiffsPage` pagination and `DiffDetailPage` polling have stale-response races.
**AC:** a shared `useFetch` (abortable, with `{data,loading,error}`) replaces the ad-hoc pattern on the high-traffic pages; error state is distinct from empty state; pagination/poll requests are ordering-safe.

### Story N-24 · First UI component/page tests + web CSP
> **Priority:** P1 · **Size:** M · **Hat:** FEATURE
> **Finding:** Zero component/page tests — the M-15 async polling and M-18 form/error-state fixes have no regression guard; the web `index.html` ships no CSP (only the desktop renderer has one).
**AC:** RTL tests for DiffDetail (release-note polling), Settings (button types/error states), Diffs (pagination/service filter); add a CSP `<meta>`/header to the web build.

### Story N-25 · Destructive-action safety
> **Priority:** P2 · **Size:** S · **Hat:** FEATURE
> **Finding:** `SettingsPage` `deleteWebhook`/`deleteScan` are destructive with no confirmation and uncaught (unhandled promise rejection on failure).
**AC:** confirm before delete; catch and surface failures; also fix remaining hardcoded `#fff`/`var(--blue,#…)` literals by adding the missing AIR tokens.

---

## P1/P2 — Architecture & strategic debt

### Story N-26 · Decide the PostgreSQL question (supersedes M-20)
> **Persona:** Operator / maintainer choosing a production database
> **Value:** So that the advertised PostgreSQL web mode either works with real, checked queries — or is honestly descoped
> **Priority:** P1 (biggest structural risk) · **Size:** XL · **Hat:** PREPARATORY→FEATURE (or REFACTORING if descoping) · **DoR:** READY (decision first)

**Finding:** `sqlx::AnyPool` forfeits compile-time query checking, its `?`→`$n` translation is broken at runtime (42601, M-20), and the "portable" queries never actually ran on Postgres. The portability tax (TEXT-only, LCD SQL) is paid on every query while only SQLite ships.

**Decision (SPIKE N-26-T0):** (a) **Commit to Postgres** — replace `AnyPool` with `enum DbPool { Sqlite(SqlitePool), Pg(PgPool) }` (or feature-gated backends) so each backend gets real, checked queries; make the `rust-postgres` CI job gating. OR (b) **Descope Postgres** — remove the claim from README/SOLUTION_DESIGN/compose/enterprise docs, keep SQLite-only, delete `AnyPool` portability constraints over time.
**Tasks:** T0 SPIKE decision (a/b); then either the per-backend pool migration (large, staged) or the descope + doc/CI cleanup.

**✅ DONE (Option C — keep `AnyPool`, fix the placeholder layer):** Rather than the per-backend pool rewrite (a) or descoping (b), the root cause — sqlx `Any` not translating `?`→`$N` for Postgres — is fixed at the query layer. `radar-api/src/db.rs` rewrites the final SQL string (`pg()` + `q!`/`qs!`/`qa!` macros) to `$N` when the pool is Postgres, and is a no-op (borrow) on SQLite. ~337 call sites converted. The `rust-postgres` CI job is **re-enabled as gating** (`cargo test --all` against Postgres 16). Three cross-backend bugs this surfaced were also fixed (sample_rate f32/f64 decode, missing FK parent in a scans test, two env-var test races). **Verified green on PR #1: the full radar-api suite passes on real Postgres.** Commits `f337501` + `7cc5373`.

### Story N-27 · Decompose `lib.rs` to the documented module map
> **Priority:** P2 · **Size:** L · **Hat:** REFACTORING
> **Finding:** `radar-api/src/lib.rs` is 5,393 lines (3× the next module) and its flat 27-file layout has drifted from the `SOLUTION_DESIGN §4.5` module architecture (diffs/evidence/impact/policy/artifacts/catalog/authz/audit); the 4,500-line test module lives inside it.
**AC:** router/state extracted from `lib.rs`; test module moved to `tests/`; module layout reconciled with the design doc (or the doc updated to match); no behavior change (REFACTORING — existing tests unchanged).

### Story N-28 · Dedupe webhook retry loop
> **Priority:** P2 · **Size:** S · **Hat:** REFACTORING
> **Finding:** `deliver_webhook_event` vs `retry_pending_delivery` duplicate ~90 lines of retry logic; `delivered_at` is bound before the retry loop (wrong timestamp on later attempts).
**AC:** single retry helper; `delivered_at` recorded at actual delivery time. (The diff-persistence duplication is handled by N-3.)

### Story N-29 · Per-org settings (from M-21)
> **Priority:** P2 · **Size:** M · **Hat:** FEATURE
> **Finding:** `settings` table (migration 007) has no `org_id`; `PUT/GET /v1/settings` are global.
**AC:** migration adds `org_id` to `settings` (and reworks the digest-dedup keys); handlers scope to the caller's org; cross-org isolation test.

---

## P2 — CI/CD, tests, hygiene

### Story N-30 · Narrow or schedule the Postgres CI job
> **Priority:** P2 · **Size:** S · **Hat:** REFACTORING
> **Finding:** `rust-postgres` is `continue-on-error: true` and known-failing, so new PG regressions are indistinguishable from M-20 and a full compile+test burns per push for an ungated signal.
**AC:** either reduce it to `sqlx migrate run` only (gating, should pass post-M-7b) until N-26 lands, or remove it and track N-26. Gates migration regressions again.

**✅ DONE:** Narrowed to migrations-only under this story, then **re-widened to the full gating `cargo test --all`** once N-26 fixed the query layer. The job now gates both migration and query-layer portability on Postgres 16.

### Story N-31 · Desktop into CI
> **Priority:** P2 · **Size:** S · **Hat:** FEATURE
> **Finding:** `radar-desktop` has no `lint`/`typecheck` scripts so `pnpm --recursive lint` skips it; it's never type-checked or `electron-vite build`-compiled until a release tag (M-16's criterion was not actually met); root `package.json` `lint` calls a nonexistent `radar-desktop lint`.
**AC:** add `typecheck` (and lint) scripts to radar-desktop; CI runs typecheck + an unsigned `electron-vite build`; fix the root lint script.

### Story N-32 · JS supply-chain scanning
> **Priority:** P2 · **Size:** S · **Hat:** FEATURE
> **Finding:** `cargo audit`+SBOM exist, but there's no `pnpm audit` and no Dependabot/Renovate — an Electron app's dependency tree is unscanned.
**AC:** add a `pnpm audit` CI step (advisory or gating) and a `.github/dependabot.yml` for cargo + npm + actions.

### Story N-33 · Make E2E assertive
> **Priority:** P2 · **Size:** M · **Hat:** FEATURE
> **Finding:** Playwright specs `test.skip` when the backend is absent (always, in CI) and use `if (isVisible())` guards — smoke tests that pass while features are broken.
**AC:** a docker-compose-backed E2E job (the compose file exists) with real assertions on at least one core journey (submit diff → see blast radius); remove the tolerant guards.

### Story N-34 · Branch protection
> **Priority:** P2 · **Size:** S · **Hat:** (process)
> **Finding:** History shows direct pushes to `main`.
**AC:** protect `main` requiring `Rust`, `Node / pnpm`, `Coverage`, `OpenAPI spec` + 1 review.

### Story N-35 · Release signing & cross-platform
> **Priority:** P2 · **Size:** M · **Hat:** FEATURE
> **Finding:** Unsigned NSIS installer (SmartScreen), Windows-only `dist -- --win` while README promises DMG/AppImage, `electron-updater` with no feed (ties to N-21).
**AC:** code-signing wired (or documented as internal-only), macOS/Linux artifacts produced, update feed configured. (Do only if public distribution is a goal.)

### Story N-36 · Repo hygiene
> **Priority:** P2 · **Size:** S · **Hat:** REFACTORING
> **Finding:** Tracked `drift.db`/`radar.db` (data-leak + merge noise), orphaned `radar-sdk-node`/`radar-sdk-python` (in no workspace, Node SDK untested), duplicate root `Dockerfile.api` and stray `package-lock.json`, and the still-untracked `AGENTS.md`/`docs/cliff-notes.md`/`docs/APIRadar_Icon.png`.
**AC:** untrack + gitignore the `.db` files; decide the SDKs' fate (adopt into a workspace with CI, or remove); delete the dead Dockerfile/lockfile; commit or remove the intended untracked docs.

---

## P2 — Lower-severity correctness & polish (N-37 · grouped cleanup)
> **Priority:** P2 · **Size:** M · **Hat:** FEATURE/REFACTORING · one story, itemized:
- **Deterministic proto/graphql output** — replace `HashMap`/`HashSet` iteration with ordered maps so change ordering is stable (OpenAPI already uses IndexMap).
- **Mojibake** — `release_notes.rs:276` has three U+FFFD replacement chars in user-facing Markdown; fix the literal.
- **`usage_event.recorded_at` index** — add it (retention purge + blast-radius recency scan the hottest table).
- **`ApiError::BadRequest` → 422 vs documented 400** — pick one and make errors.rs, openapi.yaml, and tests agree (docs currently say 422 after M-19; confirm the variant name isn't misleading).
- **`audit.rs` StatusCode bypass** — `list_audit_events` returns `Result<_, StatusCode>`, bypassing the ApiError JSON envelope + logging; align it.
- **`batch` honors `.radar.yml`** — run the policy engine (fail_mode, block_on, override) and post a decision, matching `check`.
- **Dead api-testing output** — either expose the `apitesting` YAML suite via a CLI flag or remove the dead `generate_both` path.
- **CSV runner zombie jobs** — sweep `pending`/`running` rows stranded by a restart (webhook-outbox analog) or mark them failed on startup.
- **Scanner path fabrication** — verb-prefix + naive pluralization invents `GET /categorys`; lower confidence or guard against non-matching pluralization.
- **Splash TOCTOU** — desktop writes splash HTML to a fixed temp path; use a `data:` URL or userData dir.
- **`proto.rs` field-rename kind** — emits `FieldRemoved` for a rename; use a rename/`FieldAdded`+`FieldRemoved` pair or a dedicated kind.

---

## Execution order

- **Wave 1 (P0):** N-1, N-2, N-3 — small, localized, currently-silent bugs. Do first.
- **Wave 2 (P1 core):** N-4…N-8 (diff completeness) ‖ N-16…N-18 (scanner) ‖ N-9…N-15 (api robustness) ‖ N-19…N-21 (desktop) ‖ N-22…N-25 (UI). Rust serialized; UI/desktop parallel.
- **Wave 3 (strategic):** N-26 (Postgres decision — do the SPIKE early even if the work lands later) → N-27/N-28/N-29.
- **Wave 4 (P2 CI/hygiene/polish):** N-30…N-37.

Cargo builds run one at a time (disk). Non-Rust stories (UI, desktop, CI, docs, hygiene) can proceed concurrently with the Rust queue.

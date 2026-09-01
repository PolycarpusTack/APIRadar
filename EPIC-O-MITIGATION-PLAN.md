# EPIC O — Full-Application Review Mitigation (2026-09-01)

> **Objective:** Close every gap from the 2026-09-01 nine-workspace review so the product's core promise — detect every Breaking Change and attribute it to the Consumers it hits — holds without silent blind spots.
> **Mode:** HARDENING (no new features; FEATURE-as-fix / REFACTORING / PREPARATORY only)
> **Tracer Bullet?:** NO — builds on the delivered A–N path
> **Source:** 2026-09-01 five-agent full review (radar-core+scanner, radar-api, radar-cli, radar-ui+desktop, SDKs/action/CI); top findings independently re-verified in source; both test suites green at review time
> **Framework:** GPM v2.1 + Backlog Builder v5.1 (v5.2 extension conventions: per-task Confidence, ST0 SPIKE on Low, PARALLEL/SEQUENTIAL story flags)
>
> **Definition of Done (EPIC additions to Global DoD):**
> - [ ] Every P0 story lands its failing regression test before the fix (Red→Green), and the "self-diff yields zero changes" corpus (O-3) passes for all three spec formats
> - [ ] `cargo test --workspace` + `pnpm --recursive test` + the new SDK CI lane (O-10) green; clippy `-D warnings` clean
> - [ ] No finding is closed by deleting its symptom from docs — docs and behaviour must agree (O-21)
>
> **SLO additions:**
> - `radar-core – identical-spec diff change count = 0 over the realistic fixture corpus (all 3 formats)`
> - `radar-api – service-token mode authorized-request success = 100% over auth integration tests`
> - `Evidence – collection-only Consumer coverage > 0 after `radar scan --collection` in the E-6 demo E2E`
>
> **Risk Assessment:**
> - *Med:* O-7 (destructuring/closure attribution) can balloon — mitigated by scoping to direct destructure of an awaited client call; deeper flows stay a logged TD item
> - *Med:* O-19 (AI Gateway) depends on 01-Unified-AI-Gateway being reachable from this suite — `Accepted until 2026-10-01 — gateway readiness unverified — Owner: Yannick`
> - *Low:* diff-engine fixes may re-classify past diffs; severities are computed per run, no stored data migrates
>
> **Assumptions Ledger:**
> - [High-Impact] The open EPIC N tail (see §Relationship) remains the tracker for the findings it already covers — EPIC O does not duplicate them
> - Postman v2.1 remains the only collection format in scope
>
> **ADRs:** no new ADRs expected; O-4 documents the CallerOrg service-token resolution inline (extends F-02 rationale)
> **Smoke Test Story:** O-23
> **Runbook Link:** `docs/runbook.md` — updated by O-21 (rollback section is currently wrong)

---

## Domain Glossary

Existing terms only: Producer, Consumer, Blast Radius, Breaking Change, Evidence, Fail Mode. No new terms.

---

## Relationship to EPICs M / N — why these findings are new

The 2026-07 review produced EPIC M (delivered) and EPIC N (delivered except the open tail). The 2026-09-01 findings split into three buckets:

1. **Already tracked — open EPIC N stories.** Not re-planned here; close them under their N IDs:

| 2026-09 finding | Open story |
|---|---|
| Param schemas compared by type label only; non-2xx excluded; `application/json` exact-match | **N-8** |
| Unclamped pagination (audit, csv results, decisions, acks) | **N-13** |
| No HTTP timeouts in explain/register/jira/postman clients; byte-slice panics | **N-15** |
| Scanner precision/reach beyond O-6/O-7's specific bugs | **N-16/N-17/N-18** |
| A11y gaps (BatchComparePanel rows, aria-labels, htmlFor) | **N-22** |
| Unguarded fetch races (DiffDetail, ConsumerDetail, Audit) | **N-23** |
| Silent `.catch(() => {})` on destructive mutations | **N-25** |
| Webhook delivery/retry loop duplication + swallowed DB errors | **N-28** |
| `/v1/settings` not org-scoped (retention purge blast radius) | **N-29** |
| Desktop/E2E/CI structural stories | **N-31/N-33/N-34/N-35** |

2. **Regressions and leftovers from delivered fixes** — new by construction, not missed twice: service-token 401s came in with the F-02 CallerOrg refactor (010939a); the composed-variant context bug came in with the N-5 oneOf diff; the root-kind catch-all is what the N-4 array fix left behind; `batch_compare` is the one path 64ec5ea didn't convert; the runbook/README contradictions were created by a7d6956 and N-26 landing after those docs were written.

3. **Surfaces no prior review covered:** SDK packaging/CI, radar-action internals, GraphQL scalars, Postman normalization, CSV Runner logic.

---

## Priority tiers

- **P0** — the product gives a wrong answer (silent missed/misclassified Breaking Change, corrupted Evidence) or an advertised mode is broken.
- **P1** — correctness/security hardening that affects real users but doesn't falsify the core promise.
- **P2** — documentation truth, hygiene, grouped cleanup.

---

# P0 — Wrong answers & broken modes

### Story O-1 · Root-schema kind changes must diff · **PARALLEL**

> **Persona:** Producer paginating a list endpoint
> **Value:** So that changing a response/request's top-level shape (array→object, object→string, object→oneOf) is reported as the Breaking Change it is, instead of nothing
> **Priority:** P0 (silent miss on the most common breaking list-endpoint change)
> **Size:** M · **Dependencies:** none · **DoR:** READY

**Finding:** `radar-core/src/diff.rs:880-883` — after nullable/enum/composed checks, only (Object,Object) and (Array,Array) pairs recurse; every other pair hits `_ => return` with zero changes emitted.

**Acceptance Criteria**
```gherkin
Given a response schema changes from array-of-User to {items, total}
When the specs are diffed
Then a Breaking TypeChanged is reported at the response root

Given a schema changes from object to string, or from object to a oneOf wrapper
Then a Breaking TypeChanged is reported

Given base and head schemas have the same top-level kind
Then behaviour is unchanged (existing suite stays green)
```

**Tasks**
| ID | Hat | Goal | Confidence | Budget |
|---|---|---|---|---|
| O-1-T1 | FEATURE | Failing tests: array→object, object→string, object→oneOf, oneOf→anyOf each yield a root Breaking TypeChanged | High | ≤2 500 |
| O-1-T2 | FEATURE | Compare `type_label_from_kind` (incl. composed-kind labels) at the root before the object/array match; emit TypeChanged/Breaking on mismatch | High | ≤2 500 |

---

### Story O-2 · Request context survives composed variants · **PARALLEL**

> **Persona:** Producer using oneOf request bodies
> **Value:** So that optional→required inside a request variant is Breaking, not Safe
> **Priority:** P0 (Breaking classified Safe — the one failure the gate exists to prevent)
> **Size:** S · **Dependencies:** none · **DoR:** READY

**Finding:** `radar-core/src/diff.rs:162-164` — `is_request_context()` string-matches `request_body`/`request_body.`, but `diff_composed_variants` (:950) builds prefixes as `request_body[0]`, which matches neither; required-changes inside request variants take the response branch.

**Acceptance Criteria**
```gherkin
Given a request body oneOf variant where a field flips optional→required
When the specs are diffed
Then RequiredChanged is reported with Breaking severity and request wording

Given the same flip in a response oneOf variant
Then severity remains Safe
```

**Tasks**
| ID | Hat | Goal | Confidence | Budget |
|---|---|---|---|---|
| O-2-T1 | FEATURE | Failing tests for request-vs-response required flips inside oneOf/anyOf variants | High | ≤1 500 |
| O-2-T2 | REFACTORING→FEATURE | Thread an explicit `is_request: bool` through the schema-diff recursion (kill the prefix string-sniffing); fix classification | High | ≤2 500 |

---

### Story O-3 · GraphQL scalar parity + self-diff-zero corpus · **PARALLEL**

> **Persona:** GraphQL Producer with `scalar DateTime`
> **Value:** So that unchanged schemas never report Breaking Changes and the gate stops crying wolf
> **Priority:** P0 (permanent false Breaking for effectively all real GraphQL schemas)
> **Size:** M · **Dependencies:** none · **DoR:** READY

**Finding:** `radar-core/src/graphql.rs:300-326` — `diff_type` has no `(Scalar, Scalar)` arm; the catch-all emits `TypeChanged`/Breaking for every custom scalar on every diff, including identical specs. Root cause of survival: the whole diff suite tests minimal specs; no "realistic spec vs itself" test exists for any format.

**Acceptance Criteria**
```gherkin
Given an SDL containing custom scalars, directives-in-use, unions, and enums
When diffed against itself
Then zero changes are reported

Given a scalar declaration is removed
Then OperationRemoved (existing convention) is still reported

Given the realistic OpenAPI and proto corpus fixtures
When each is diffed against itself
Then zero changes are reported (regression corpus, all 3 formats)
```

**Tasks**
| ID | Hat | Goal | Confidence | Budget |
|---|---|---|---|---|
| O-3-T1 | FEATURE | Failing tests: scalar-bearing SDL self-diff ≠ 0 today; add realistic self-diff corpus fixtures for OpenAPI/GraphQL/proto under `fixtures/` | High | ≤2 500 |
| O-3-T2 | FEATURE | Add `(Scalar, Scalar)` no-op arm (and any other same-kind identity arms the corpus exposes); wire corpus into `cargo test` | High | ≤2 000 |

---

### Story O-4 · Service-token auth mode works again · **PARALLEL**

> **Persona:** Operator deploying with `RADAR_SERVICE_TOKEN` (docs-advertised mode)
> **Value:** So that a correct bearer token authorizes requests instead of 401ing every CallerOrg endpoint
> **Priority:** P0 (advertised deployment mode broken; F-02 regression)
> **Size:** M · **Dependencies:** none · **DoR:** READY

**Finding:** `radar-api/src/auth.rs:763-768` validates the token but never inserts `JwtClaims`; `lib.rs:535-539` sets `SingleTenantMode(false)` when a token is configured, so `CallerOrg::resolve` returns `None` → 401 on ~all `/v1` handlers. `release_notes.rs` only survives because it still uses the pre-F-02 `OrgExt` pattern (which O-4 also retires).

**Acceptance Criteria**
```gherkin
Given a router built with only RADAR_SERVICE_TOKEN configured
When a request carries the correct bearer token
Then CallerOrg endpoints (services, diffs, consumers, webhooks) respond 2xx

When a request carries a wrong or missing token
Then 401

Given release-notes endpoints
Then they use the same CallerOrg extractor as every other handler
```

**Tasks**
| ID | Hat | Goal | Confidence | Budget |
|---|---|---|---|---|
| O-4-T1 | FEATURE | Failing integration test booting the router in service-token mode and hitting a CallerOrg route (the missing test class) | High | ≤2 000 |
| O-4-T2 | FEATURE | Service-token branch resolves to an explicit CallerOrg (single-tenant semantics), by inserted claims or a dedicated variant — no silent wildcard strings (keep F-02's guarantee) | High | ≤2 500 |
| O-4-T3 | REFACTORING | Migrate `release_notes.rs` off `OrgExt` onto `CallerOrg`; delete `OrgExt` | High | ≤2 000 |

---

### Story O-5 · `radar scan` never skips collection Evidence · **PARALLEL**

> **Persona:** Consumer team whose repo is Postman-collections-only
> **Value:** So that their Evidence is posted and their coverage counts, instead of a silent exit 0
> **Priority:** P0 (Evidence silently dropped → Blast Radius undercounts)
> **Size:** S · **Dependencies:** none · **DoR:** READY

**Finding:** `radar-cli/src/main.rs:1069-1071` — `if records.is_empty() { return Ok(()) }` sits before the `--collection` loop; zero code call-sites aborts collection posting entirely.

**Acceptance Criteria**
```gherkin
Given a source dir with no scannable call sites and a valid --collection file
When radar scan runs
Then collection Evidence is posted and the summary reports it

Given neither call sites nor collections yield records
Then the command says so explicitly (no silent success)
```

**Tasks**
| ID | Hat | Goal | Confidence | Budget |
|---|---|---|---|---|
| O-5-T1 | FEATURE | Failing test: collection-only scan posts Evidence (first test for `scan` — build the minimal test seam it needs) | High | ≤2 500 |
| O-5-T2 | FEATURE | Move the early-return below the collection path; add explicit "nothing found" output | High | ≤1 500 |

---

### Story O-6 · Scanner records the real HTTP verb · **PARALLEL**

> **Persona:** Consumer calling `fetch(url, {method: "POST"})`
> **Value:** So that Evidence attaches to the operation actually called, not a phantom GET
> **Priority:** P0 (Evidence attributed to the wrong operation corrupts Blast Radius both ways)
> **Size:** S · **Dependencies:** none · **DoR:** READY

**Finding:** `radar-scanner/src/lib.rs:387-392` — bare `fetch`/`axios` calls are hard-coded to GET; the options object's `method` is never read.

**Acceptance Criteria**
```gherkin
Given fetch("/orders", { method: "POST" })  (and the axios config equivalent)
Then Evidence records POST /orders

Given fetch("/orders") with no options
Then GET /orders (unchanged)

Given a dynamic/unresolvable method expression
Then no operation is recorded rather than a guessed one
```

**Tasks**
| ID | Hat | Goal | Confidence | Budget |
|---|---|---|---|---|
| O-6-T1 | FEATURE | Failing tests: fetch/axios with literal method option (TS + Python `requests.post` parity check) | High | ≤2 000 |
| O-6-T2 | FEATURE | Read the literal `method` property from the second argument; unresolvable → skip | High | ≤2 500 |

---

### Story O-7 · Evidence from destructuring and closures · **SEQUENTIAL after O-6**

> **Persona:** Consumer writing modern TS (`const {phone} = await usersApi.getUserById(id)`)
> **Value:** So that the dominant access style produces field Evidence at all
> **Priority:** P0 (whole Consumer codebases produce zero field Evidence today)
> **Size:** L · **Dependencies:** O-6 (same module) · **DoR:** READY

**Finding:** `radar-scanner/src/lib.rs:557` binds the operation to the destructured variable name (never accessed as a member), and the scope map (:624-654) misses accesses inside nested callbacks.

**Acceptance Criteria**
```gherkin
Given const { phone, email } = await usersApi.getUserById(id)
Then field Evidence phone and email is recorded for GET /users/{id}

Given const user = await usersApi.getUserById(id); orders.forEach(o => use(user.phone))
Then field Evidence phone is recorded (closure lookup falls back through enclosing scopes)
```

**Tasks**
| ID | Hat | Goal | Confidence | Budget |
|---|---|---|---|---|
| O-7-T1 | FEATURE | Failing tests: object-pattern destructure (TS + Python tuple/attr equivalent noted or excluded), nested-callback access | High | ≤2 500 |
| O-7-T2 | FEATURE | Emit destructured identifiers as field paths for the awaited call's operation (direct destructure only; re-exported bindings are TD) | Med | ≤3 500 |
| O-7-T3 | FEATURE | Scope-chain fallback: walk enclosing scopes on lookup miss | Med | ≤2 500 |

**TD Created:** deep flows (destructure-then-pass-to-helper) logged, not chased.

---

### Story O-8 · Postman URLs normalize to spec operations · **PARALLEL**

> **Persona:** Consumer whose collection uses `{{base_url}}/users/{{userId}}`
> **Value:** So that collection Evidence matches `/users/{id}` and parameterized routes stop showing zero coverage
> **Priority:** P0 (collection Evidence silently contributes nothing for parameterized routes)
> **Size:** M · **Dependencies:** none · **DoR:** READY

**Finding:** `radar-scanner/src/lib.rs:994-1025` strips only *leading* `{{var}}`; mid-path variables stay as `{{userId}}` and literal IDs (`/users/12345`) are never templatized (unlike the code scanner's `normalize_http_path`). Also: schemeless relative URLs lose their first segment; host-only URLs become a path; unnamed items are dropped.

**Acceptance Criteria**
```gherkin
Given raw URL {{base_url}}/users/{{userId}}
Then the operation is /users/{userId} → normalized to match /users/{id} matching rules

Given /users/12345
Then numeric segments templatize exactly as the code scanner does (shared normalizer)

Given an item without a name
Then its request is still parsed
```

**Tasks**
| ID | Hat | Goal | Confidence | Budget |
|---|---|---|---|---|
| O-8-T1 | FEATURE | Failing tests: mid-path vars, numeric IDs, relative URL, host-only URL, unnamed item | High | ≤2 500 |
| O-8-T2 | REFACTORING→FEATURE | Extract one shared path normalizer used by both the code scanner and the Postman parser; convert `{{var}}` → `{var}` anywhere in the path | Med | ≤3 000 |

---

### Story O-9 · Python SDK: installable and honest · **PARALLEL**

> **Persona:** FastAPI Consumer team adopting the SDK
> **Value:** So that `pip install` works, operations are route patterns (not raw paths), and flushes don't stall the event loop
> **Priority:** P0 (SDK never installable; Evidence it would send is per-ID noise)
> **Size:** M · **Dependencies:** none · **DoR:** READY

**Findings:** `pyproject.toml:1-3` names a nonexistent build backend; `middleware.py:132-139` reads `scope["route"]` before awaiting the app (routing sets it during); `middleware.py:46-64` flushes synchronously from the async path.

**Acceptance Criteria**
```gherkin
Given python -m build
Then a wheel builds (backend setuptools.build_meta)

Given a routed request /users/123 under a FastAPI route /users/{user_id}
When the middleware records after the app call, without hand-injected scope
Then the operation is GET /users/{user_id}

Given a full batch triggers flush
Then the request path does not block on network I/O (flush on a worker thread)
```

**Tasks**
| ID | Hat | Goal | Confidence | Budget |
|---|---|---|---|---|
| O-9-T1 | FEATURE | Fix build backend; failing packaging check (build in test/CI); fix `str | None` annotation vs 3.9 floor; drop or implement the `django` extra | High | ≤1 500 |
| O-9-T2 | FEATURE | Failing test using a real Starlette app (no scope injection) → record after `await self.app(...)` | High | ≤2 500 |
| O-9-T3 | FEATURE | Move flush I/O off the event loop (worker thread / queue), lock held only for queue swap | Med | ≤2 500 |

---

### Story O-10 · SDKs get a CI lane · **SEQUENTIAL after O-9** *(PREPARATORY for every future SDK change)*

> **Persona:** Maintainer changing an SDK
> **Value:** So that broken packaging or types can never ship silently again (three of this review's criticals lived here)
> **Priority:** P0 (systemic gap that produced shipped-broken artifacts)
> **Size:** S · **Dependencies:** O-9 (must be green to gate) · **DoR:** READY

**Finding:** No workflow touches `radar-sdk-node` or `radar-sdk-python`; `pnpm-workspace.yaml` excludes them; `radar-sdk-node/src/index.d.ts:13` has an invalid constructor return annotation every TS consumer trips on.

**Acceptance Criteria**
```gherkin
Given the ci workflow on a PR touching either SDK
Then node --test runs, tsc validates index.d.ts, python -m build + unittest run — all gating

Given index.d.ts
Then it compiles under tsc --strict
```

**Tasks**
| ID | Hat | Goal | Confidence | Budget |
|---|---|---|---|---|
| O-10-T1 | FEATURE | Fix `index.d.ts` (+ add `repository`/`files` to package.json); add a tsc check test | High | ≤1 500 |
| O-10-T2 | FEATURE | `sdk` CI job (path-filtered): node tests + d.ts typecheck + python build/test | High | ≤2 000 |

---

### Story O-11 · Desktop dev mode loads the real dev server · **PARALLEL**

> **Persona:** Contributor running `pnpm dev:desktop`
> **Value:** So that dev mode opens the app instead of a blank window
> **Priority:** P0 (dev workflow broken outright)
> **Size:** S · **Dependencies:** none · **DoR:** READY

**Finding:** `radar-desktop/electron/main/index.ts:585` hardcodes `localhost:5173`; electron-vite serves 5181 (`electron.vite.config.ts:60`); the IPC token guard (:548) also only trusts 5173.

**Acceptance Criteria**
```gherkin
Given pnpm dev:desktop
Then the window loads process.env.ELECTRON_RENDERER_URL and get-api-token succeeds from that origin

Given a packaged build
Then behaviour is unchanged (file:// path)
```

**Tasks**
| ID | Hat | Goal | Confidence | Budget |
|---|---|---|---|---|
| O-11-T1 | FEATURE | Use `ELECTRON_RENDERER_URL` for load + trusted-origin check; assert dev-launch smoke in N-31's lane when it lands | High | ≤1 500 |

---

### Story O-12 · CSV Runner obeys its own controls · **PARALLEL**

> **Persona:** Operator re-running a CSV batch of non-idempotent POSTs
> **Value:** So that unticking "Retry on 5xx" actually disables retries, and a historical run exports its own rows
> **Priority:** P0 (duplicate-POST risk; wrong data attached to exports)
> **Size:** S · **Dependencies:** none · **DoR:** READY

**Findings:** `radar-ui/src/components/CsvRunnerPanel.tsx:272-310` — `runBatch` memoized without `captureBody`/`enableRetry`; `:222-229` — historical-run results pair with the currently loaded CSV's rows by index instead of persisted `row_data` by `row_number`.

**Acceptance Criteria**
```gherkin
Given the retry toggle is changed and Run is clicked
Then the posted request reflects the current toggle values

Given CSV A is loaded and a historical run of CSV B is opened
When Failed Rows is exported
Then rows come from run B's persisted row_data, matched by row_number
```

**Tasks**
| ID | Hat | Goal | Confidence | Budget |
|---|---|---|---|---|
| O-12-T1 | FEATURE | Failing RTL tests for both behaviours (first tests for this component) | High | ≤2 500 |
| O-12-T2 | FEATURE | Fix dep array; key results by `row_number` and prefer `row_data` for historical runs | High | ≤2 000 |

---

### Story O-13 · `batch` joins the policy engine · **PARALLEL**

> **Persona:** Team with `fail_mode: warn` running batch checks in CI
> **Value:** So that batch verdicts follow the same policy, produce the same audit trail, and can't fail CI a policy would allow
> **Priority:** P0 (policy bypass + missing decision audit rows)
> **Size:** M · **Dependencies:** none · **DoR:** READY

**Finding:** `radar-cli/src/main.rs:936-941` — batch exits on raw breaking/error counts; no `.radar.yml`, no `decide()`, no `POST /v1/policy-decisions` (CLAUDE.md mandates posting after checks).

**Acceptance Criteria**
```gherkin
Given fail_mode: warn and a batch row with a Breaking Change
Then exit code follows the policy verdict (warn → 0) and a policy decision is posted per row with API configured

Given no API configured
Then batch still applies the local policy file to its exit code
```

**Tasks**
| ID | Hat | Goal | Confidence | Budget |
|---|---|---|---|---|
| O-13-T1 | FEATURE | Failing tests: batch + warn policy exit code; decision posted per row (first `batch` tests, incl. the CSV parser's escaped-quote gap) | High | ≤2 500 |
| O-13-T2 | FEATURE | Route each row through `decide()` + decision posting; aggregate exit = worst verdict | High | ≤3 000 |

---

### Story O-14 · The PR comment tells the truth about coverage blocks · **PARALLEL**

> **Persona:** Developer whose PR is blocked by InsufficientCoverage (FIT-01)
> **Value:** So that the comment explains the actual reason (no Evidence) instead of asserting evidence exists, and names the configured override label
> **Priority:** P0 (actively misleads at the exact moment of blocking; erodes trust in the gate)
> **Size:** S · **Dependencies:** none · **DoR:** READY

**Finding:** `radar-cli/src/github.rs:304-310` renders "At least 1 high-confidence evidence record present" for zero-evidence blocks and hardcodes `drift-ack` even when `allow_override_with` differs. The correct explanation already exists on stderr (`main.rs:521-535`) but never reaches the PR.

**Acceptance Criteria**
```gherkin
Given a Verdict of InsufficientCoverage with zero Evidence
Then the PR comment states no coverage Evidence exists and how to add it, mirroring the stderr explanation

Given allow_override_with: custom-label
Then the comment names custom-label
```

**Tasks**
| ID | Hat | Goal | Confidence | Budget |
|---|---|---|---|---|
| O-14-T1 | FEATURE | Failing comment-rendering tests for both cases (suite already covers this module — extend it) | High | ≤1 500 |
| O-14-T2 | FEATURE | Thread the verdict reason + configured label into the comment body | High | ≤2 000 |

---

# P1 — Correctness & security hardening

### Story O-15 · `batch_compare` uses the pinned SSRF client · **PARALLEL** · Size S

> **Finding:** `radar-api/src/diffs.rs:1294-1358` still does validate-then-fetch with a plain client — the DNS-rebinding window 64ec5ea closed on the other four outbound paths.
> **AC:** batch compare fetches via `ssrf_pinned_client`; a rebinding-shaped test (validated addr ≠ connect addr) fails closed.
> **Tasks:** O-15-T1 FEATURE failing test + swap to pinned client (High, ≤2 000).

### Story O-16 · Playground never persists bearer tokens locally · **PARALLEL** · Size S

> **Finding:** `radar-ui/src/pages/PlaygroundPage.tsx:63-65,224-229` — any server failure (including auth) silently falls back to writing sandbox `bearer_token` values into localStorage.
> **AC:** local fallback stores envs without token values (or session-only in memory); the UI states tokens aren't saved locally; existing stored tokens are purged on load.
> **Tasks:** O-16-T1 FEATURE failing test: fallback path stores no `bearer_token` (High, ≤2 000); O-16-T2 FEATURE strip+purge+copy (High, ≤2 000).

### Story O-17 · radar-action: honest inputs, hardened shell · **PARALLEL** · Size M

> **Finding:** `radar-action/action.yml:27-30` `fail-mode` is declared/documented/dead; `:115-125` interpolates inputs directly into bash (fork-PR injection surface); `:141` jq without null fallback; cache never short-circuits the build.
> **AC:** `fail-mode` either drives the policy for the run or is removed everywhere (action.yml + README + getting-started doc — no dead documented inputs); all `${{ inputs.* }}` pass via `env:` and are quoted (the token already does this — same pattern); smoke test loses its false comment and asserts outputs.
> **Tasks:** O-17-T1 FEATURE decide+wire-or-delete fail-mode incl. docs (Med, ≤2 500); O-17-T2 FEATURE env-quote all inputs + jq fallbacks (High, ≤2 000); O-17-T3 FEATURE make test-radar-action assert outputs/exit codes rather than continue-on-error (High, ≤2 000).

### Story O-18 · Proto streaming + cross-format enum taxonomy · **PARALLEL** · Size M

> **Finding:** `radar-core/src/proto.rs:411-435` erases `stream` so unary↔streaming rpc changes vanish (and corrupts types starting with "stream"); GraphQL emits `FieldRemoved` for enum-value removal instead of `EnumValueRemoved`, and enum/union additions are Safe while OpenAPI says NonBreakingRisky.
> **AC:** streaming-ness change → Breaking OperationRemoved-or-TypeChanged per convention; `streamer` parses intact; GraphQL/proto enum changes emit the same ChangeKind + severity as OpenAPI (one taxonomy — downstream filters on `enum_value_removed` see all formats).
> **Tasks:** O-18-T1 FEATURE failing tests both engines (High, ≤2 500); O-18-T2 FEATURE parse streaming as part of the rpc signature + align kinds/severities (Med, ≤3 000).

### Story O-19 · AI calls route through 01-Unified-AI-Gateway · **SEQUENTIAL after ST0** · Size M · **Confidence: Low → ST0 SPIKE**

> **Finding:** `radar-cli/src/ai_provider.rs:108,154` (and the documented duplicate in `radar-api/src/ai.rs`) call api.anthropic.com/OpenAI directly — a suite non-negotiable violation; the Copilot path posts a raw token to an endpoint shape that likely never worked, failing silently; model IDs hardcoded.
> **ST0 SPIKE (timeboxed):** confirm 01-Unified-AI-Gateway's endpoint contract, auth, and availability from this suite; decide keep-or-delete for the Copilot path. Output: a one-page contract note → re-estimate T2.
> **AC (post-spike):** all AI calls target the gateway base URL (env-overridable); provider/model configurable; a dead Copilot path is removed rather than silently failing.
> **Tasks:** O-19-ST0 SPIKE (Low, ≤1 500); O-19-T1 FEATURE route through gateway + config (Med, ≤3 000).

### Story O-20 · Desktop residuals: splash, iframe popups, CSP parity · **PARALLEL** · Size S

> **Finding:** splash HTML written to fixed world-writable `%TEMP%` path (`main/index.ts:466`, TOCTOU spoofing); `allow-popups-to-escape-sandbox` on the Scalar iframe (`PlaygroundPage.tsx:512`) lets a malicious spec pop OS-browser URLs; desktop CSP lacks `object-src`/`base-uri` and allows CDN fetches the app doesn't need offline.
> **AC:** splash loads from `userData` (or data: URL); iframe drops `allow-popups-to-escape-sandbox`; desktop CSP gains the missing directives without breaking the Playground (N-20 already proved the CSP/Playground interaction — retest it).
> **Tasks:** O-20-T1 FEATURE splash relocation (High, ≤1 500); O-20-T2 FEATURE sandbox + CSP tighten with Playground E2E rerun (Med, ≤2 000).

---

# P2 — Documentation truth & grouped cleanup

### Story O-21 · Docs tell the truth · **PARALLEL** · Size S

> **Finding:** `docs/runbook.md:143` claims "no down-migrations" (false since a7d6956); `README.md:226-229` warns Postgres is unusable (false since N-26, contradicts CI); README + CLAUDE.md workspace maps omit radar-action and both SDKs; `HELP.md` — suite-mandatory at `building`+ — does not exist; CLI PR-comment footer/UI sidebar still say v0.1/v0.2.
> **AC:** runbook rollback section documents `sqlx migrate revert` + when to prefer restore; README recommends Postgres for production per CLAUDE.md; both workspace maps list all nine workspaces; `HELP.md` exists and covers install→check→scan→dashboard; version strings come from crate/package metadata.
> **Tasks:** O-21-T1 FEATURE runbook+README+maps (High, ≤2 000); O-21-T2 FEATURE write HELP.md + fix version strings (High, ≤2 000).

### Story O-22 · Grouped minor cleanup (single PR, one reviewer pass) · **SEQUENTIAL last** · Size M

> One task per line, all Hat REFACTORING unless noted, each ≤1 000:
> - Delete dead `check --spec` flag; fix `--base/--head` help text (files only, no git refs) — or implement git-ref reading (FEATURE, decide at pickup)
> - Delete or implement `DriftConfig.collection_paths` (documented no-op) — align README + policy-reference docs
> - Remove dead `radarToken` localStorage plumbing in UI (or implement token entry; decide at pickup)
> - `--json` + `--post-comment` stdout purity; include verdict/diff_id/blast radius in `--json` output (FEATURE)
> - Add `--json` to `rule list`, `scan`, `register`, `explain`; unify `--token`/env fallback across subcommands (FEATURE)
> - `severity_str` → use core serialization; main.rs consumes `radar_cli_lib` instead of re-declaring modules
> - Delete dead `apitesting.rs` YAML path or wire it to a command (decide at pickup)
> - radar-ui: stop hand-copying design tokens — import `_TEMPLATE/design-tokens.css` per suite rule; collapse desktop splash palette duplication
> - Pin third-party GitHub Actions by SHA (at minimum: softprops/action-gh-release, dawidd6/action-download-artifact); recommend a tagged radar-action ref in its README
> - Scanner: fix stale S1/S2 doc comment; Postman spurious `body` field emission; `response.` prefix double-match

---

### Story O-23 · Smoke test: the whole promise, end to end · **SEQUENTIAL after O-1..O-8** · Size M

> **Persona:** Platform engineer proving the gate before rollout
> **Value:** So that one E2E proves detect → attribute → decide with the exact shapes this review found broken
> **Priority:** P0 gate for EPIC close
> **DoR:** READY after wave 1

**Acceptance Criteria**
```gherkin
Given the demo fixtures extended with: a list endpoint changing array→paginated object,
      a oneOf request required-flip, an SDL with custom scalars,
      and a collection-only Consumer using {{base_url}}/users/{{userId}}
When radar scan --collection runs for the Consumer and drift check runs for the Producer
Then coverage for the Consumer is > 0
And the diff reports both Breaking Changes and zero false Breaking Changes
And the policy decision posts, and the PR comment names the configured override label
```

**Tasks:** O-23-T1 FEATURE extend fixtures + E2E in `radar-cli/tests/` reusing the E-6 harness (High, ≤3 500).

---

## Execution order & parallelism

| Wave | Stories | Notes |
|---|---|---|
| 1 (PARALLEL) | O-1, O-2, O-3, O-4, O-5, O-6, O-11, O-12, O-14 | All independent; diff-engine trio shares files — one owner, sequential commits within the trio |
| 2 | O-7 (after O-6), O-8, O-9, O-13, O-15, O-16 | O-9 independent; start anytime |
| 3 | O-10 (after O-9), O-17, O-18, O-20, O-19-ST0 | O-19-T1 only after spike verdict |
| 4 | O-23 smoke, O-21 docs | O-23 gates EPIC close |
| 5 | O-22 grouped cleanup | Single PR, last |

**Also in flight, tracked under EPIC N (do not duplicate):** N-8, N-13, N-15, N-16..N-18, N-22..N-25, N-28, N-29, N-31, N-33..N-35 — the review reconfirmed all of these; closing EPIC O without the N tail does not fully close the application's gaps.

---

## Phase Gate Checklist (EPIC O exit)

- [ ] All P0 stories green with their Red→Green regression tests in history
- [ ] Self-diff-zero corpus (O-3) wired into `cargo test` for all three formats
- [ ] Service-token integration test in the auth suite; `OrgExt` deleted
- [ ] SDK CI lane gating on PRs; both SDKs build + test green
- [ ] O-23 smoke E2E green on SQLite and Postgres CI lanes
- [ ] Runbook, README, workspace maps, HELP.md consistent with reality (O-21)
- [ ] Clippy `-D warnings`, fmt, `pnpm --recursive lint` clean; no new TD items without a ledger entry
- [ ] DEVELOPMENT_ORDER / DEVELOPMENT_PLAN / README cluster table updated in sync (suite rule)

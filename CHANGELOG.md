# Changelog

All notable changes to Radar Monitor are documented in this file.

Format follows [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).
Versions follow [Semantic Versioning](https://semver.org/).

---

## [0.3.0] — 2026-08-31

Security and reliability work from a second full-codebase review, plus the
dependency backlog that had accumulated while CI was red. **One default changes
behaviour — see Breaking below before upgrading.**

> **Version note:** the `0.2.1` entry below was written but the crate versions
> were never bumped — every crate stayed at `0.2.0` and no git tag was created.
> This release moves all crates and packages to `0.3.0` and tags it, so the
> changelog and the code agree again.

### Breaking

- **`policy.block_on` now defaults to `any_break`** (was `active_consumers`).

  Previously, a breaking change exited **0** whenever the blast radius was
  empty — which on a fresh install it always is, because nobody has
  instrumented anything yet. Radar was at its most permissive exactly when a
  team had the least protection, and it reported success while doing it.

  Repos relying on the old default **will start failing** on breaking changes.
  To keep the previous behaviour:

  ```yaml
  # .radar.yml
  policy:
    block_on: active_consumers
  ```

- **Existing dashboard sessions are invalidated.** Session and OIDC CSRF-state
  tokens are now signed with purpose-derived keys, so tokens issued before this
  release no longer validate. Users sign in again once; in-flight logins simply
  restart.

- **`radar-api` refuses to start** when no authentication is configured and
  `--bind` is reachable from outside the machine. Set `RADAR_JWT_SECRET`,
  `RADAR_SERVICE_TOKEN`, or `RADAR_REQUIRE_AUTH=true`; bind to `127.0.0.1` for
  desktop/local use; or set `RADAR_ALLOW_UNAUTHENTICATED=true` to keep serving
  an open API deliberately.

### Security

- **OIDC login could mint a cross-tenant session.** The callback checked
  neither the HTTP status nor the parse result of the `userinfo` response and
  fell back to a default identity, producing an empty `org_id` — this system's
  "no isolation" wildcard — in a valid 24-hour session cookie. Any
  non-conforming body triggered it, including a `403 insufficient_scope` from a
  user who declined the `profile` scope. The login now fails closed, and a
  session is never signed without a usable org.
- **id_token verification hardened** — the algorithm is pinned to an allowlist
  of asymmetric algorithms instead of being read from the token header; the
  issuer is pinned from discovery; and an OIDC `nonce` is now sent and asserted,
  making an intercepted id_token non-replayable.
- **Token confusion made structurally impossible** — session tokens and CSRF
  state tokens previously shared one HS256 key and were kept apart only by
  which fields serde happened to require. Each purpose now derives its own key.
- **Session cookie `Secure` flag** is settable via `RADAR_COOKIE_SECURE`, rather
  than inferred from the redirect URI — which silently dropped the flag behind a
  TLS-terminating proxy.

### Reliability

- **A panic in one request no longer kills the server.** The release profile set
  `panic = "abort"`, so any handler panic terminated the process; with no
  unwinding, no catch-panic layer could intervene. Removed, and `CatchPanicLayer`
  added, so a panic becomes a 500 for that request.
- **New `insufficient coverage` verdict.** With `block_on: active_consumers`, an
  empty blast radius for a service with *no evidence at all* is now reported as
  unresolved rather than as a pass, with guidance on how to fix it.

### Dependencies

- Coordinated upgrades that Dependabot could not raise as single PRs, because
  each crate only compiles alongside its siblings: **hmac 0.13 + sha2 0.11**,
  **metrics 0.24 + metrics-exporter-prometheus 0.18**, and **tree-sitter 0.26 +
  the python and go grammars**. HMAC signatures and spec hashes are unchanged
  and now pinned by known-answer tests.
- **crossbeam-epoch 0.9.20** for RUSTSEC-2026-0204.
- **TLS unified on rustls.** `sqlx` and `lettre` moved off native-tls, removing
  OpenSSL from the dependency graph entirely — so no system OpenSSL is needed to
  build. Trust remains anchored in the OS certificate store, so a Postgres
  server presenting a private CA still verifies.

### Build and CI

- **The toolchain is pinned to an exact release** (`1.98.0`). It tracked the
  floating `stable` channel while CI runs `clippy -- -D warnings`, so a new
  clippy release turned a lint into a hard build failure with no change to this
  repo. `main` had been unbuildable since 2026-07-02 as a result.
- **`cargo audit` is advisory on pull requests and blocking on a nightly
  schedule.** It compares `Cargo.lock` against the live RustSec database, so its
  result changes with time rather than with the code under review; a new
  advisory against any transitive dependency used to red-light every open PR.
- **PostgreSQL TLS is now tested** — a CI job connects with `sslmode=verify-full`
  against a server whose CA is installed in the system trust store, and asserts
  both that the session is encrypted and that a hostname mismatch is rejected.
- **`/metrics` has its first test.** The endpoint had no coverage at all.

### Fixed

- `ApiError::BadRequest` renamed to `ApiError::Unprocessable` — it renders as
  HTTP 422, not 400.
- `radar-desktop/out/` build artefacts untracked (they were committed before the
  ignore rule landed).
- Dead `drop(pool)` calls and an unused `State` extractor removed from the auth
  middleware.

---

## [0.2.1] — 2026-07-02

Security, correctness, and reliability hardening from a full-codebase review (EPIC M). No new user-facing features; existing behaviour is unchanged except where noted.

### Security

- **Catalog sync credential exfiltration + SSRF fixed** — `POST /v1/catalog-sources` previously read *any* client-named environment variable and sent it to a client-supplied URL. `token_env` is now restricted to names prefixed `RADAR_CATALOG_TOKEN_`, and catalog fetches go through the SSRF guard with redirects disabled and a request timeout.
- **Desktop sidecar now requires a bearer token** — the Electron app generates a per-session token, passes it to the `radar-api` sidecar as `RADAR_SERVICE_TOKEN`, and attaches it from the renderer. A website visited in the user's browser can no longer read or mutate the local drift database.
- **Playground stored-XSS fixed** — sandbox-environment values interpolated into the API-explorer iframe are now HTML/JSON-escaped.
- **Multi-tenant isolation** — release notes, acknowledgements, scheduled scans, subscriptions, AI test generation, summary, digest preview, spec-version listing, and the audit log now enforce the caller's org via a shared `require_org_owned` guard (cross-org access returns 403).
- **Hardening** — database URL credentials are redacted in logs; secret comparisons are constant-time; the rate limiter now keys on the real socket peer (X-Forwarded-For trusted only behind `RADAR_TRUST_PROXY`) instead of an unvalidated bearer token; share tokens are now random (not derivable from the diff id); Slack "View Diff" links use `RADAR_PUBLIC_BASE_URL`.

### Fixed

- **Breaking-change detection** — the OpenAPI diff now correctly flags request-body and parameter `optional → required` changes, `requestBody.required` flips, and dropped `application/json` content types as breaking; path-item-level parameters are diffed; renaming a path-template variable (`/users/{id}` → `/users/{userId}`) no longer reports a phantom operation add/remove.
- **Protobuf diff** — `parse_proto` now errors on non-proto input instead of silently reporting "no changes", and parses `oneof` members and `map<>` fields.
- **Scheduled scans** — a scan now diffs against the immediately previous spec (an off-by-one previously diffed against an empty or two-generations-old spec).
- **PostgreSQL migrations** — migrations `014`–`017` no longer use the SQLite-only `strftime()` default, so a fresh PostgreSQL database migrates cleanly (see *Known limitations*). New migration `033` adds `webhook_delivery.created_at` and hot-path indices; delivery listing orders by `created_at` instead of the SQLite-only `rowid`.
- **Scanner accuracy** — the Postman collection parser recurses nested folders, normalizes `:var` path params to `{var}`, strips query strings/BOM, and no longer emits false field paths from `pm.response.*`; `.tsx` files parse with the TSX grammar, plain-JS extensions are scanned, and API-call operations are attributed to the enclosing function.
- **CLI** — in fail-open mode with an API error, a valid label override and `block_on: never` are now honoured; all HTTP clients have timeouts; GitHub PR comment lookup paginates past 100 comments.
- **Desktop** — single-instance lock (a second launch no longer kills the first instance's sidecar), tree-kill on quit, PID-reuse guard, a recoverable startup dialog, and post-startup crash detection.
- **Dashboard** — release-note generation polls the async status endpoint and renders the result; public share links and audit diff links work under the `/app` base path; the Settings form no longer fires unintended saves; the diffs list is paginated and the service filter works; the home timeline and diff table use a consistent time zone.
- **API** — blast-radius evidence writes are idempotent; batch ingestion is transactional and maps constraint violations to 4xx; `mask_token` no longer panics on multi-byte tokens.

### Changed

- Negative/unbounded `limit` query parameters are clamped consistently across list endpoints (previously dumped the whole table on SQLite / errored on PostgreSQL).
- New operator environment variables: `RADAR_SERVICE_TOKEN` (bearer required on `/v1` when set), `RADAR_TRUST_PROXY`, `RADAR_PUBLIC_BASE_URL`, and the `RADAR_CATALOG_TOKEN_*` allowlist for catalog `token_env`.
- CI/release fixes: pnpm installed before its cache step, the desktop installer builds with the correct config (sidecar bundled), Playwright targets the served port, workspace formatted, `quinn-proto` bumped for a RUSTSEC advisory, and the composite `radar-action` builds its binary to a findable path.

### Known limitations

- **PostgreSQL runtime queries** — migrations apply on PostgreSQL, but the runtime query layer has an unresolved `sqlx` `Any`→PostgreSQL placeholder-translation issue; the shipped path (SQLite / desktop) is unaffected. The `Rust (Postgres)` CI job runs but is non-blocking until this is resolved.
- **Per-org settings** — `PUT/GET /v1/settings` remain global (auth-gated); org scoping needs a schema migration.

---

## [0.2.0] — 2026-05-26

### Added

#### CSV Runner (EPIC L + Hardening Sprint 2)
- **Bulk API execution from CSV** — upload a spreadsheet, define a URL/method/headers/body template using `{{column_name}}` placeholders, and Radar fires one request per row; live progress counter in the UI.
- **Per-row retry with backoff** — transient 5xx responses and network errors are retried up to 3 times (delays: 0 s → 1 s → 4 s); 4xx responses are definitive and never retried.
- **Response body capture (opt-in)** — tick "Capture response body" before a run to store the first 10 KB of each row's response in the database; expand any result row to read it inline without re-running.
- **Results export** — download the full run results as a CSV (row number, HTTP status, duration ms, error, response body).
- **Automatic retention** — the hourly background job now purges terminal-status (`completed`, `failed`, `cancelled`) CSV run rows older than the configured retention window.

#### Signals & Integrations (EPIC K)
- **Outbound webhooks** — register HTTP endpoints to receive real-time notifications when a new diff is stored; CRUD at `POST /v1/webhooks`, delivery history at `GET /v1/webhooks/{id}/deliveries`, manual test-fire at `POST /v1/webhooks/{id}/test`.
- **Slack Block Kit** — configure a Slack incoming webhook to receive formatted diff summaries with blast-radius counts.
- **Scheduled spec scanning** — configure recurring spec scans at `POST /v1/scheduled-scans`; Radar fetches the spec URL on a schedule and runs a diff automatically.
- **Public diff permalink** — generate a shareable link to any diff detail page via `POST /v1/diffs/{id}/share`.
- **Weekly email digest** — opt-in digest of breaking changes, delivered to configured addresses each Monday.
- **GitHub Status Check** — post a commit status (success/failure) to GitHub alongside the PR comment.

#### Non-technical UX (EPIC J)
- **Compare Specs panel** — paste or upload two spec files directly in the browser; no CLI needed.
- **Consumer registration form** — register consumers from the Consumers page without leaving the dashboard.
- **First-run wizard banner** — guided onboarding banner surfaces on empty state for new installations.
- **Inline jargon tooltips** — hover over domain terms (Blast Radius, Consumer, Producer, etc.) for a plain-English definition.
- **Post-create service nudge** — after registering a service, a contextual prompt takes you straight to Compare Specs.
- **Release Notes status workflow** — draft → reviewed → published transitions with timestamps.
- **Evolution Rules callout** — surfaces active evolution rules (field renames, type widening) inline on the diff detail page.

#### Test coverage (Hardening Sprint 2)
- 10 new happy-path smoke tests for EPIC K: webhook CRUD + scheduled-scan CRUD.
- Retention and retry unit tests for the CSV Runner subsystem.

### Changed
- SSRF guard in the CSV Runner now blocks IP literals in the `100.x`, `169.254.x`, and `fd00::/8` ranges in addition to RFC1918 addresses.
- `impact_evidence` expiry now also covers rows from the collection-file source type.

### Fixed
- Stale `drift-ui` directory in `radar-desktop/node_modules/` causing `@electron/rebuild` to fail on `pnpm run dist`.
- Webhook and scheduled-scan test URLs now use IP literals to bypass DNS resolution in offline CI environments.

---

## [0.1.0] — 2026-05-22

Initial release — EPICs A through I.

### Included
- OpenAPI 3.x / GraphQL SDL / Protobuf 3 diffing via `radar check`
- Blast-radius computation backed by three evidence sources (OTel, tree-sitter, Postman)
- Policy engine with `block_on: never | any_break | active_consumers` and `fail_mode: closed | warn | open`
- GitHub PR comments with evidence table and verdict badge
- AI release notes and per-consumer migration guides (`radar explain`)
- AI Postman test generation from Jira tickets (`radar generate-tests`)
- API Playground (Scalar) with shared sandbox environments
- Consumer registry with organization isolation
- OIDC authentication (Google, Okta, Azure AD)
- Electron desktop app with bundled `radar-api` sidecar
- Docker Compose self-host configuration with PostgreSQL
- Prometheus metrics endpoint
- Demo scenario fixtures (payments-api v1→v2 breaking change)

# Changelog

All notable changes to Radar Monitor are documented in this file.

Format follows [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).
Versions follow [Semantic Versioning](https://semver.org/).

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

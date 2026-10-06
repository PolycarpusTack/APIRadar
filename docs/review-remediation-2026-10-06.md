# Code review remediation — 6 October 2026

Execution mode: MAINTENANCE. Scope: the eight confirmed review findings. Preserve existing workspace changes; no deployment, commit, or unrelated refactoring is included.

## Plan

Each task follows failing regression test → minimum correction → refactor and verification. Behavior corrections use the FEATURE hat; test-only compatibility uses REFACTORING. Tasks run sequentially, with no concurrent Cargo commands.

| Task | Hat | Acceptance criteria / regression | Status |
| --- | --- | --- | --- |
| REVIEW-1 | REFACTORING | Scan tests compile with URL-scoped history. A newer snapshot from another URL cannot become the selected base. | Complete: 4 focused tests pass |
| REVIEW-2 | FEATURE | Given another organisation's Producer or Consumer ID, every Evidence ingestion endpoint rejects the request before writing any batch item. Owned IDs continue to work. | Complete: 16 regressions pass |
| REVIEW-3 | FEATURE | Given repeated comparison labels, changed spec content produces a new Diff without mutating earlier Spec Versions. Identical comparisons remain idempotent, including batch comparisons. | Complete: API history and batch regressions pass |
| REVIEW-4 | FEATURE | Matching collection Evidence contributes to Blast Radius at medium confidence, including auto-registered Consumers without subscriptions. Unrelated, expired, or foreign Evidence does not contribute. | Complete: matching/filtering regressions and the UI badge test pass |
| REVIEW-5 | FEATURE | Adding a required request property is a Breaking Change; adding an optional request property or response property is safe. | Complete |
| REVIEW-6 | FEATURE | Request nullability narrowing and response nullability widening are Breaking Changes. The opposite directions are safe. | Complete |
| REVIEW-7 | FEATURE | Scalar constraints are compared at root schemas and nested array items as well as object properties, without duplicate Change records. | Complete |
| REVIEW-8 | FEATURE | Removing a GraphQL default from an existing non-null argument or input field is a Breaking Change; nullable fields are not made required by default removal. | Complete: core 84 existing + 18 new tests pass |

All tasks are ready: reproduction evidence and affected paths were established during the review. Each is bounded to the named behavior and its direct entry points. REVIEW-1 unlocks API regression execution; REVIEW-2 precedes Evidence matching; REVIEW-3 precedes final API checks. REVIEW-5–8 are independently testable in radar-core. Stop for approval if a fix requires a new product policy or unrelated architectural change.

## Verification and completion gate

Run focused regressions after each correction. Then run Rust workspace tests and Clippy sequentially, formatting checks for changed Rust files, UI tests/type checks/lint, and both SDK test suites. SQLite is available locally; PostgreSQL compatibility must remain intact, but a PostgreSQL run requires an available test database. Record actual results and any environmental limitations below.

DoD: acceptance criteria pass; no behavior outside these fixes changes intentionally; Spec Version history stays immutable; organisation boundaries and Evidence source semantics remain explicit; no secrets logged; no new dependencies unless justified. No feature flag is planned for corrections to existing safety guarantees. Rollback is limited to the newly authored changes, preserving pre-existing edits.

## Results

All eight corrections are implemented. Regression-first testing added 51 tests: 32 API tests (including scan and batch history coverage), 18 core classification tests, and one UI test. The security review guidance shaped the two-resource ownership checks, whole-batch rejection tests, and defensive Coverage filtering.

| Check | Result |
| --- | --- |
| `cargo test --workspace --offline --jobs 1 -- --quiet` | 560 tests passed; no failures |
| UI Vitest suite | 152 tests passed |
| UI TypeScript and ESLint | Passed |
| UI production build | Passed; existing Vite config warning only |
| Desktop TypeScript | Passed |
| Node SDK | 4 tests passed |
| Python SDK | 23 tests passed; existing pytest-asyncio configuration warning |
| Clippy (`--all-targets --all-features`, `-D warnings`) | Passed |
| Rust formatting and Git whitespace checks | Passed |
| OpenAPI YAML and ingestion contracts | Parsed with unique keys; all five endpoints declare 202/403 correctly |

Validation limits: no PostgreSQL test connection or TLS-test URLs were configured. The PostgreSQL TLS tests therefore returned without connecting; SQL migrations and API regressions ran against isolated SQLite databases. Performance tests ran in debug/reporting mode, not the release SLO enforcement lane. Packaged Electron and browser E2E tests were not run. Those deployment-specific checks remain for CI/release validation.

Collection Evidence with a known operation but no field paths conservatively matches property Changes within that operation. Known field paths still require a field match; another operation never matches. Collection Evidence retains the static-source age behavior, with expiry always enforced and `max_age_days` available to restrict it further. Reading it does not create duplicate Evidence records.

Spec Version IDs now fingerprint content and format as well as readable refs. No database migration is required. The submitted-Diff endpoint has no base spec; its metadata-only base identity includes the submitted Change report so a different report cannot return an old cached Diff. Existing stored history is retained, but previously overwritten snapshots cannot be reconstructed by this fix.

No historical data is deleted. If an older deployment contains forged Usage Events or Call Sites, those schemas lack ingestion-actor provenance; distinguishing them from legitimate rows requires separate audit or backup Evidence.

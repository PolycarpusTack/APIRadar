# API Contract Radar Monitor — Claude Code Guide

## Framework

This project uses the AI-Native Software Delivery framework. All task execution must follow:

- `Agents/gpm-v2.1.md` — GPM methodology (phases, prompt types, phase gates)
- `Agents/backlog-builder-v5.1.md` — story/task templates, DoR/DoD
- `Agents/core-specification-v1.md` — shared principles, modes, global DoD

Current execution mode: DELIVERY (EPIC F+)
Active stories: see `DEVELOPMENT_PLAN.md` — EPIC E complete, EPIC F is next.

---

## Domain language

Always use terms from the Domain Glossary in `DEVELOPMENT_PLAN.md`. Never use synonyms:

| Correct term    | Do NOT use                          |
|-----------------|-------------------------------------|
| Producer        | provider, publisher                 |
| Consumer        | client, subscriber                  |
| Blast Radius    | impact, affected services           |
| Breaking Change | breaking, breaking update           |
| Evidence        | signal, indicator                   |
| Fail Mode       | policy mode, gate mode              |

---

## Workspace structure

```
radar-core/       Shared Rust types (ChangeKind, Severity, Consumer, Diff, …)
radar-cli/        CLI binary (clap 4) + radar_cli_lib (pub: github, render, api_client, policy)
radar-api/        axum HTTP service; run with --db sqlite:PATH or --db postgres://...
radar-scanner/    tree-sitter code scanner + Postman Collection v2.1 parser
radar-ui/         Vite 6 + React 19 web renderer (shared with desktop)
radar-desktop/    Electron 33 shell (wraps radar-ui, spawns radar-api sidecar)
fixtures/         Demo scenario fixtures for E-6 integration tests
docs/             Runbook, enterprise plan, openapi.yaml
```

---

## Commands

### Rust

```sh
cargo build                               # build all Rust crates
cargo test                                # run all tests
cargo test -p radar-cli --test demo_scenario  # run E-6 integration tests
cargo clippy -- -D warnings               # lint (CI fails on warnings)
cargo fmt --all                           # format
```

**Important:** Never run multiple `cargo` commands in parallel — disk fills up fast. Always run them sequentially.

### Node / pnpm

```sh
pnpm dev:ui                               # start radar-ui Vite dev server (localhost:5173)
pnpm dev:desktop                          # start radar-desktop in Electron dev mode
pnpm build:ui                             # build radar-ui static bundle
pnpm --recursive lint                     # lint all workspaces
```

---

## Database

| Context            | Connection string                              |
|--------------------|------------------------------------------------|
| Electron / local   | `--db sqlite:drift.db`                         |
| Production (web)   | `--db postgres://user:pass@host/drift`         |
| CI                 | `DATABASE_URL=sqlite:ci-test.db`               |

Run migrations:

```sh
sqlx migrate run --source radar-api/migrations
```

Migration compatibility rules:
- Use `TEXT` for IDs and timestamps — never `SERIAL`, `BIGSERIAL`, or `TIMESTAMPTZ`
- All migrations must work on **both** SQLite and PostgreSQL
- Test locally against SQLite; CI and production use PostgreSQL
- Current migrations: 001–013 (013 adds `catalog_source` to consumer)

---

## Key architectural patterns

### ChangeKind string representation
`ChangeKind::as_str()` is defined in `radar-core/src/models.rs`. Use it in all crates — do **not** duplicate the match arm locally.

### Evidence writing
All evidence flows through `impact_evidence` (append-only, migration 011). Three source types:
- `runtime_usage` — from `POST /v1/usage/events`; confidence high/medium based on recency
- `static_call_site` — from `POST /v1/call-sites`; confidence medium (S2, operation known) or low (S1)
- `collection_file` — from `POST /v1/evidence/collection`; confidence medium; deterministic ID for idempotency

### Consumer auto-registration
`POST /v1/consumers/upsert` registers a consumer by name without requiring `repo_url`. Used by the collection file scanner. Idempotent on `(org_id, name)`.

### Policy engine
`radar-cli/src/policy.rs` — `decide()` takes `(changes, policy, fail_mode, has_active_consumers, has_label_override, api_error)`. Always post the result to `POST /v1/policy-decisions` after `drift check`.

### Library target
`radar-cli` exposes `radar_cli_lib` as a `[lib]` target. Integration tests in `radar-cli/tests/` import from `radar_cli_lib`. Keep `lib.rs` to just `pub mod` declarations for: `api_client`, `github`, `policy`, `render`.

---

## TDD order (mandatory per framework)

1. Write a failing test
2. Implement the minimum code to make it pass
3. Refactor

Never skip step 1. No exceptions.

---

## Hat declarations (one hat per task)

Every task must be labelled with exactly one hat before implementation begins:

| Hat           | When to use                                          |
|---------------|------------------------------------------------------|
| FEATURE       | New functionality visible to end users               |
| REFACTORING   | Restructuring without any behavior change            |
| PREPARATORY   | Restructuring that enables an upcoming FEATURE       |

---

## Security rules

- All Electron `BrowserWindow` instances: `contextIsolation: true`, `nodeIntegration: false`
- The `radar-api` sidecar must bind to `127.0.0.1` only (never `0.0.0.0`) in desktop mode
- Never log tokens, passwords, or database credentials
- `ANTHROPIC_API_KEY` comes from environment variables only — never hardcode or commit it
- Treat `radar-desktop/src/main/` (Node process) as an untrusted boundary: validate all IPC messages

---

## CI notes

- The `docker` job only runs on push to `main` (requires no signing certs)
- The Electron desktop build is skipped in CI for the same reason
- `pnpm-lock.yaml` is committed and must never be `.gitignore`d
- Clippy warnings are treated as errors (`-D warnings`) in CI
- Test counts (approximate): radar-scanner 27, radar-api 42, radar-cli (unit + integration) ~55+

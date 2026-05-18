# API Contract Radar Monitor — Claude Code Guide

## Framework

This project uses the AI-Native Software Delivery framework. All task execution must follow:

- `Agents/gpm-v2.1.md` — GPM methodology (phases, prompt types, phase gates)
- `Agents/backlog-builder-v5.1.md` — story/task templates, DoR/DoD
- `Agents/core-specification-v1.md` — shared principles, modes, global DoD

Current execution mode: PROTOTYPE (EPIC A) → DELIVERY (EPIC B+)
Active stories: see `DEVELOPMENT_PLAN.md`

---

## Domain language

Always use terms from the Domain Glossary in `DEVELOPMENT_PLAN.md`. Never use synonyms:

| Correct term    | Do NOT use                          |
|-----------------|-------------------------------------|
| Producer        | provider, publisher                 |
| Consumer        | client, subscriber                  |
| Blast Radius    | impact, affected services           |
| Breaking Change | breaking, breaking update           |

---

## Workspace structure

```
radar-core/       Shared Rust types (Change, Consumer, Diff, etc.)
radar-cli/        CLI binary (clap 4)
radar-api/        axum HTTP service; run with --db sqlite:PATH or --db postgres://...
radar-scanner/    tree-sitter background worker
radar-ui/         Vite 6 + React 19 web renderer (shared with desktop)
radar-desktop/    Electron 33 shell (wraps radar-ui, spawns radar-api sidecar)
```

---

## Commands

### Rust

```sh
cargo build                               # build all Rust crates
cargo test                                # run all tests
cargo clippy -- -D warnings              # lint (CI fails on warnings)
cargo fmt --all                           # format
```

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

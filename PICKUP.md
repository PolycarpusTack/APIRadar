# Pickup Prompt — API Contract Radar Monitor (continue EPIC N)

> Paste the block below as your first message to a fresh Opus 4.8 session in this repo.
> Snapshot taken 2026-07-02, end of the Postgres/N-26 session.

---

## Where we are

- **Branch:** `main`. Last session merged **PR #1 (EPIC N Postgres + quality wave)** — merge commit `54ea20f`. The `quality/epic-n` branch is deleted; local `main` is synced with `origin/main`.
- **Headline result:** PostgreSQL now actually works. sqlx `Any` doesn't translate `?`→`$N` for Postgres (was 42601 errors); fixed at the query layer in `radar-api/src/db.rs` (`pg()` + `q!`/`qs!`/`qa!` macros). The `rust-postgres` CI job runs the full `cargo test --all` against Postgres 16 as a **gating** check and is green. Do not regress this — keep it gating.
- **Backlog of record:** `QUALITY-BACKLOG.md` (EPIC N, stories N-1..N-37). Done stories are marked; the open set is below.
- **App status:** working application. Rust workspace + pnpm workspace both build/test/lint clean; CI fully green on the last push.

## What's merged (don't redo)

N-1..N-8 (diff-engine correctness), N-16/17/18 (scanner), N-19..N-25 (desktop + UI), **N-26** (Postgres query layer), N-30/31/32/36-partial/37-partial (CI + hygiene). Details in `QUALITY-BACKLOG.md` and memory `project-epic-n-status`.

## Open work (priority order)

**P1 — API robustness (recommended next wave):**
- **N-9** Ingestion honesty — FK failures must return 4xx, not be silently counted as "accepted".
- **N-10** SSRF DNS-rebinding + non-blocking DNS resolution.
- **N-11** Per-org weekly digest.
- **N-12** Share-token intent + shared-view severity parity.
- **N-13** Uniform pagination clamping — `clamp_pagination` helper exists (utils.rs); apply it across *all* list handlers.
- **N-14** Scheduled-scan serialization.
- **N-15** CLI remaining timeouts + panic guard.

**P2 — structural / correctness tail:**
- **N-27** Decompose `radar-api/src/lib.rs` (~5.4k lines) toward the SOLUTION_DESIGN §4.5 module map; move the test module to `tests/`. Pure REFACTORING.
- **N-28** Dedupe webhook retry loop; fix `delivered_at` recorded before the retry loop.
- **N-29** Per-org `settings` (add `org_id`, scope handlers, cross-org isolation test).
- **N-37 tail:** deterministic proto/graphql ordering, batch policy parity, dead api-testing output, csv zombie jobs, scanner path fabrication, splash TOCTOU, proto rename kind, audit StatusCode bypass.
- **N-33** Assertive compose-backed E2E · **N-34** Branch protection (recommendation) · **N-35** Release signing.

**Needs a decision from Yannick (ask before acting):**
- **N-36** Orphaned `radar-sdk-node` / `radar-sdk-python` — in no workspace, Node SDK untested. Adopt into a workspace with CI, or delete?

**Housekeeping:**
- 8 Dependabot GitHub-Actions bump PRs open (**#2–#9**) from the N-32 config. Review/merge as a batch (they're low-risk action version bumps; confirm CI green on each).

## How to work here (guardrails)

- **Framework is mandatory:** `Agents/gpm-v2.1.md`, `backlog-builder-v5.1.md`, `core-specification-v1.md`. One **Hat** per task (FEATURE / REFACTORING / PREPARATORY). **TDD**: failing test first, always.
- **Domain glossary is strict** (see `CLAUDE.md`): Producer, Consumer, Blast Radius, Breaking Change, Evidence, Fail Mode — never synonyms.
- **Cargo is serialized** — never run `cargo build`/`test`/`clippy` in parallel; C: fills up. `cargo clean` if disk gets tight.
- **Cross-backend rules** (learned the hard way this session — every one is a real bug class): queries use `?` and go through `q!`/`qs!`/`qa!`; `REAL` is f64 on SQLite but f32 on Postgres (read tolerant); Postgres enforces FKs (fixtures must insert parents); tests must never mutate process-global env vars (extract pure fns). Migrations must work on both backends (TEXT ids/timestamps).
- **CI gates:** clippy `-D warnings`, fmt, SQLite tests, Postgres tests, coverage ≥65%. Run `cargo fmt --all` + `cargo clippy --all-targets -- -D warnings` before pushing. Docker/desktop jobs skip off `main` — that's expected, not a failure.
- **Git:** work on a branch, open a PR to `main`, get CI green, then merge. Commit trailer: `Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>`.

## Suggested first action tomorrow

Start the **P1 API-robustness wave**: pick up **N-9** (ingestion honesty) with a failing test that posts an event referencing a non-existent FK and asserts a 4xx + that it is *not* counted as accepted. N-9/N-10/N-13/N-15 are largely independent and can go in parallel on one branch. Batch-merge the Dependabot PRs (#2–#9) first if you want a clean CI baseline.

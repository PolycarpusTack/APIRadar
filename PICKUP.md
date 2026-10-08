# Pickup Prompt — API Contract Radar Monitor (continue EPIC N tail)

> Paste the block below as your first message to a fresh session in this repo.
> Snapshot taken 2026-10-08.

---

## Where we are

- **EPIC O is complete** (23 stories, `EPIC-O-MITIGATION-PLAN.md`). EPICs A–L and M are done as well.
- **PR #77** (`quality/epic-o-and-n-tail` → `main`) holds 37 commits: all of EPIC O plus the EPIC N tail below. They had never been pushed, so this PR is their first CI run, including the required Postgres 16 lane. **Check its CI first.** Merge it when green; if Postgres fails, fix it on the branch.
- **Local verification at PR time:** clippy clean; `cargo test --workspace` green on SQLite (radar-api 284); radar-ui 152/152 with lint + typecheck clean.
- **Backlog of record:** `QUALITY-BACKLOG.md` (EPIC N). Done stories carry a ✅ note with a date.

## Done in EPIC N (don't redo)

N-1..N-9, N-13, N-14, N-15, N-16..N-20, N-22, N-23, N-25, N-26, N-28, N-29, N-30, N-31, N-32, N-36.

## Open work (priority order)

**P1 — data exposure first:**
- **N-11** Per-org weekly digest. It currently aggregates all orgs into one email, which leaks data across orgs.
- **N-12** Share tokens are minted as a side effect of `GET /v1/diffs/:id`; the shared view also skips evolution-rule severities.
- **N-10** SSRF DNS rebinding + DNS resolution blocking the async runtime.
- **N-21** Desktop auto-update (wire it or drop `electron-updater`) + crash/process-gone logging.
- **N-17** Scanner reach (direct HTTP clients, Java/C#/Ruby).
- **N-24** Remaining UI page tests + web CSP.

**P2:** N-27 (split `radar-api/src/lib.rs`, ~5.8k lines; pure REFACTORING), N-33 (assertive compose-backed E2E), N-34 (branch protection), N-35 (release signing), N-37 tail items.

**Carried from EPIC O:** packaged-desktop Playground E2E (O-20); O-23 smoke on the Postgres lane (PR #77 covers it).

**Housekeeping:**
- About 20 Dependabot PRs are open (#54–#76). Several are **major** bumps: axum 0.8, sqlx 0.9, jsonwebtoken 11, thiserror 2, tower-http 0.6, React Router 7, Vite 8, Tailwind 4, TypeScript 7, ESLint 10, Electron 44. Don't batch-merge them; handle each major bump as its own upgrade story. Minor/patch groups (#74, #75) and action bumps (#76) are low-risk.
- `docs/cliff-notes.md` and `docs/APIRadar_Icon.png` are still untracked. Yannick decides: commit or drop.

## How to work here (guardrails)

- **Framework:** `Agents/gpm-v2.1.md`, `backlog-builder-v5.1.md`, `core-specification-v1.md`. One **Hat** per task. **TDD:** failing test first. If the code already exists, prove the test red against a stub before keeping the real code.
- **Domain glossary is strict** (`CLAUDE.md`): Producer, Consumer, Blast Radius, Breaking Change, Evidence, Fail Mode.
- **Cargo is serialized and the disk is tight.** C: runs at ~97–100%; a full build + test takes ~18 GB in `target/`. Use `-j 2` and run `cargo clean` when finished.
- **Cross-backend rules:** queries use `?` through `q!`/`qs!`/`qa!`; REAL is f64 on SQLite but f32 on PG; PG enforces FKs (fixtures insert parents); `= ?` never matches NULL; tests never mutate process env; migrations use TEXT ids/timestamps and run on both backends.
- **Git:** never push straight to `main`. Branch → PR → green CI → merge.

## Suggested first action

Check CI on PR #77 and merge it when green. Then start **N-11**: write a failing test that seeds two orgs and asserts org A's digest contains none of org B's rows.

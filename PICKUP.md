# Pickup Prompt — API Contract Radar Monitor (EPIC Q in progress)

> Paste the block below as your first message to a fresh session in this repo.
> Snapshot taken 2026-10-23.

---

## Where we are

- **Backlog of record:** [`EPIC-Q-R-PLAN.md`](./EPIC-Q-R-PLAN.md) (EPIC Q: clean, safe baseline · EPIC R: OpenAPI 3.1, diff-rule gaps vs oasdiff, Kong adapter, scanner precision). Summary tables and gate checklists are in `DEVELOPMENT_PLAN.md`. Critic pass 2 approved it. Carried follow-ups are in its **Validation history**: fix the ones named for a task before pulling that task.
- **Why:** the 2026-10-23 review and competitor comparison ([review page](https://claude.ai/artifact/NYJj2gVHhhuv3iKP9quU5C)). Named Blast Radius is unclaimed for REST/gRPC; the diff engine lags oasdiff; OpenAPI 3.1 is probably unsupported; there are no users yet.
- **PR #82** (`docs/epic-q-r-plan`) holds the plan files. Docs only; merge it when convenient.
- EPIC N items in `QUALITY-BACKLOG.md` that EPIC Q/R absorb (N-10, N-11, N-12, N-27, N-33, N-34, part of N-37) are now tracked there, not in EPIC N.

## Q-1 status

- ✅ **Q-1-T2:** EvolutionRulesPage a11y test race fixed (`03126a6`: `findByRole` for the row; reproduced red with a delayed fetch).
- ✅ **Q-1-T3:** PR #77 merged as merge commit `ffe7a55` with all 13 PR checks green. Local `main` was reset to `origin/main` after `git cherry` showed no local-only work.
- ☐ **Q-1-T1: disk headroom.** C: had ~8.5 GB free; a full Rust build needs ~18 GB. Yannick chose **clearing caches under `C:\Projects`**. A scan was started but not finished. Re-run it: find `node_modules`, `target`, `.venv`/`venv`, `__pycache__`, `.next`, `dist`, `coverage`, `.vite`, `.pytest_cache`, `.mypy_cache`, `.ruff_cache`, `.turbo`, `.gradle`. Delete only directories that `git check-ignore` confirms are ignored, and show Yannick the size list first. List folders outside a git repo (`_archive`, `_backups`, …) separately; don't delete them without asking. Verify: `df -h /c` ≥ 20 GB free.

## ⚠ New: `main` is not fully green

The **JS security audit** workflow (`pnpm audit --audit-level=high`) fails on `main`: high Electron advisories (GHSA-532v-xpq5-8h95 and others; patched ≥ 39.8.1), path `radar-desktop>electron` (33.4.11). It doesn't run on PRs, so PR #77 didn't show it. The fix is the Electron major bump (Dependabot **#63**, 33 → 44). Treat it as its own upgrade story *before* Q-2 makes checks required: sandbox/CSP, preload and `electron-builder` changes need checking, so don't merge it blind. Add it to `EPIC-Q-R-PLAN.md` as a story when you start it.

## Next actions (in order)

1. Finish **Q-1-T1** (cache cleanup above).
2. Electron upgrade (#63) so the JS security audit goes green.
3. **Q-2** branch protection (HITL, GitHub settings), **Q-3** pilot (HITL, timebox ends 2026-12-31) and **Q-7** Dependabot triage can run any time.
4. Code work in order: **Q-4** (per-org Digest; **A-7 needs Yannick's confirmation** before Q-4-T2) → **Q-5** (share action) → **Q-6** (SSRF async DNS + mapped IPv6) → **Q-8** (assertive full-stack E2E) → **Q-9** (`lib.rs` split, scripted batches).
5. EPIC R can start at **R-1** (3.1 spike) and **R-9** (scanner paths) once Q-1-T1 is done.

## Open PRs (2026-10-23)

- **#82** plan docs.
- **Dependabot**, low-risk groups (Q-7): #81 cargo minor/patch, #75 npm minor/patch, #78/#79/#52/#51 Actions, #56 base64, #55 tree-sitter, #54 criterion (dev).
- **Dependabot**, majors (one story each, see Not yet specified): #57 axum 0.8, #60 sqlx 0.9, #58 jsonwebtoken 11, #61 thiserror 2, #59 tower-http 0.6, #69 React Router 7, #68 Vite 8, #67 plugin-react 6, #70 Tailwind 4, #66 TypeScript 7, #64 ESLint 10, #65 jest-dom 7, #71 jsdom 30, **#63 Electron 44 (urgent, see above)**.
- `docs/cliff-notes.md` and `docs/APIRadar_Icon.png` are still untracked. Yannick decides (Q-10).

## How to work here (guardrails)

- **Framework:** `Agents/gpm-v2.1.md`, `backlog-builder-v5.1.md`, `core-specification-v1.md`. One **Hat** per task. **TDD:** failing test first. Tasks that end red merge with `#[ignore = "<green task id>"]`, which the green task removes (see the plan's Global rules).
- **Domain glossary is strict** (`CLAUDE.md` + plan glossary): Producer, Consumer, Blast Radius, Breaking Change, Evidence, Fail Mode, Organisation (never "tenant"), Spec Corpus, Ruleset.
- **Cargo is serialized and the disk is tight.** Use `-j 2` and run `cargo clean` when finished. Use `cargo test -p <crate> -- name_a name_b` for several filters.
- **Cross-backend rules:** queries use `?` through `q!`/`qs!`/`qa!`; REAL is f64 on SQLite but f32 on PG; PG enforces FKs (fixtures insert parents); `= ?` never matches NULL; tests never mutate process env; migrations use TEXT ids/timestamps and run on both backends.
- **UI tests on this machine:** the full Vitest suite sometimes times out 1–2 unrelated tests at 5 s under parallel load. They pass alone and in CI. Rerun the named file before suspecting a regression.
- **Git:** never push straight to `main`. Branch → PR → green CI → merge.

## Suggested first action

Re-run the cache scan for Q-1-T1, show Yannick the ignored-only size list, and delete once Yannick says go.

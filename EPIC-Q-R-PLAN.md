# API Radar — EPIC Q & R Plan

> **Framework:** Backlog Builder v5.1 + GPM v2.1 + Core Specification v1 (`Agents/core-specification-v1.md`)
> **Execution mode:** DELIVERY (both EPICs)
> **Preceding EPICs:** A–O complete; EPIC N tail partly open; EPIC P drafted, not pulled
> **Scope:** EPIC Q (Clean, safe baseline) · EPIC R (A credible diff, cheaper evidence)
> **Input:** the 2026-10-23 product review ([review page](https://claude.ai/artifact/NYJj2gVHhhuv3iKP9quU5C)), `QUALITY-BACKLOG.md` (EPIC N), `SOLUTION_DESIGN.md`
> **Generated:** 2026-10-23 · **Revised:** 2026-10-23 after critic pass 1 (see Validation history)

---

## Readiness Decision

```
READINESS: PROCEED
Health Score: Clarity 2 · Feasibility 3 · Completeness 2 = 7/9
Mode: DELIVERY — shipped product in maintenance; the work mixes FEATURE-hat security fixes with refactoring, which HARDENING does not allow
STRIDE: S none new · T Q-6 (IPv4-mapped IPv6 bypass of the SSRF filter) · R Q-5 (share action audited) · I Q-4 (cross-org Digest), Q-5 (Share Token minted on read) · D Q-6 (blocking DNS on the async runtime) · E none new
Compliance: no regime specified
Embedded directives found: none
Core Specification: read
Review: APPROVED (2026-10-23, breakdown approved as drafted)
```

| Dimension | Score | Notes |
|---|---|---|
| Clarity | 2/3 | Security and cleanup items are precise and checked against the code. Diff-gap scope depends on two spikes (R-1, R-3). |
| Feasibility | 3/3 | Existing stack and patterns; no new infrastructure except a scrubbed Kong log fixture. |
| Completeness | 2/3 | No user validation yet (Q-3 closes it); gateway choice (A-3) and per-org Digest recipients (A-7) are assumptions with Pull Gates. |

## Critical Gaps

None blocking the tracer bullet. Handled as spikes or gated assumptions rather than guessed:

- **OpenAPI 3.1 behaviour is unverified.** `radar-core` parses with `openapiv3` 2.x (3.0 model). R-1 proves or disproves the break before any fix is designed.
- **Diff-rule breadth against oasdiff is unmeasured.** R-3 produces the gap matrix; R-4..R-7 carry it as a Pull Gate.
- **Per-org Digest recipients don't exist.** Recipients come from the global `RADAR_DIGEST_RECIPIENTS`. A-7 records the proposed home; Q-4-T2 gates on Yannick confirming it.

## Risks

| Risk | Severity | Mitigation | Review |
|---|---|---|---|
| Local build needs ~18 GB in `target/`; C: had 8.5 GB free on 2026-10-23 | High | Q-1-T1 (PREPARATORY, HITL) frees space or moves `CARGO_TARGET_DIR`. It is a hard Pull Gate for the Rust tasks that need a local red/green loop (listed in its Unblocks). | 2026-11-06 |
| New diff rules change PR verdicts and could block PRs that passed before | High | R-4..R-7 land behind `RADAR_DIFF_RULESET=v2` (default `v1`); flipped only after R-10 is green. | at R-10-T2 |
| New `ChangeKind` variants break UI, SDK and CLI code that switches on the kind | Med | Contract Snapshot "ChangeKind v2 additions" below: every task that adds a kind updates all listed touchpoints; the `OpenAPI spec` check is in each Verify. | at R-4-T2 |
| Pilot teams never materialise, so Q-3 stalls | Med | Accepted until 2026-12-31 — the rest of EPIC Q/R does not depend on it except R-8 and Not-yet-specified lines — Owner: Yannick Verrydt | 2026-12-31 |
| `lib.rs` split (Q-9) breaks something subtle | Med | Pure REFACTORING hat, scripted moves in batches with the full suite green and test count unchanged after each; Q-8 assertive E2E lands first. | at Q-9-T5 |
| Major dependency bumps left open accumulate CVEs | Med | Q-7 lists them; each becomes its own story (Not yet specified). `cargo audit` already runs in CI. | 2026-12-01 |

## Domain Glossary (additions for Q & R)

All terms in `DEVELOPMENT_PLAN.md` and `CLAUDE.md` stay in force (Producer, Consumer, Blast Radius, Breaking Change, Evidence, Fail Mode, Spec Version, Diff, Change, Share Token, Digest, Organisation). Use "Organisation", never "tenant".

| Term | Definition |
|---|---|
| **Spec Corpus** | The regression oracle for the diff engine: the existing `fixtures/self-diff-corpus/` (self-diff = 0 Changes) extended with real-world spec pairs (base, head) and their expected Changes. One corpus, not two. |
| **Ruleset** | The set of Change classification rules the diff engine applies; `v1` is today's 14 change kinds, `v2` adds the R-4..R-7 rules |
| **Diff Engine** | The component that turns two Spec Versions into Changes: Radar's own `radar-core`, or (if ADR-018 so decides) oasdiff behind an adapter |
| **Gateway Adapter** | An ingestion endpoint that accepts a specific gateway's native log payload and turns it into usage Evidence without a forwarder |
| **Deprecation Signal** | An operation, parameter or field marked `deprecated: true`, or an operation whose responses declare a `Sunset` header |
| **Pilot Team** | An external team that runs Radar against its own Producer and Consumers during Q-3 |

## Assumptions Ledger

| # | Assumption | Impact | Verification |
|---|---|---|---|
| A-1 | The `EvolutionRulesPage` CI failure is a test race: line 135 uses synchronous `getByRole` for the row after awaiting only the callout | Med | Q-1-T2 red on a delayed rules fetch (confirmed by reading; critic agreed) |
| A-2 | `openapiv3` 2.x rejects or misreads common 3.1 constructs (`type: [x, "null"]`, `const`, `examples` arrays, `webhooks`) | High | R-1-T1 failing test — **early spike** |
| A-3 | Kong is the gateway Pilot Teams are most likely to run | Med | Q-3 interview answers; R-8 Pull Gate |
| A-4 | Low-risk Dependabot groups (minor/patch, GitHub Actions) merge without code changes | Low | CI green on each merge (Q-7-T1) |
| A-5 | Teams will try Radar if it names affected Consumers in a PR comment | High | Q-3 go/no-go — **early spike** |
| A-6 | A Kong 3.x http-log entry carries a stable request id or start timestamp, the Consumer (`consumer.username`/`consumer.id`), and the operation (`request.method` + `route.paths` or `request.uri`) | Med | R-8-T1 against a recorded payload |
| A-7 | Per-org Digest recipients live in a new `digest_recipient(org_id, email)` table managed through Settings; `RADAR_DIGEST_RECIPIENTS` keeps working only for the default Organisation of a single-org deployment; an Organisation with no recipients gets no Digest | High | **Yannick confirms before Q-4-T2** (Pull Gate) |

## ADR Register (new)

| ADR | Decision | Status | Owner story |
|---|---|---|---|
| ADR-017 | OpenAPI 3.1 parsing approach (`oas3` crate / 3.1→3.0 normalisation / delegate to oasdiff) | Proposed | R-1 |
| ADR-018 | Diff-rule strategy: port missing rules into `radar-core`, or run oasdiff as an optional Diff Engine | Proposed | R-3 |

## Contract Snapshot — ChangeKind v2 additions

```
CONTRACT SNAPSHOT: ChangeKind
Version: 2 (additive; v1 = 14 variants in radar-core/src/models.rs)
Rule: a variant is added only by the task that first emits it; serde snake_case name = as_str().
Touchpoints every adding task updates in the same PR:
  - radar-core ChangeKind enum + as_str()
  - docs/openapi.yaml ChangeKind enum            (CI: "OpenAPI spec")
  - radar-ui EvolutionRulesPage kind list         (rule creation)
  - radar-ui TermTooltip / kind labels
  - radar-cli explain text + release_notes templates
  - radar-sdk-node / radar-sdk-python, if they enumerate kinds
Expected additions (final names fixed by ADR-018): parameter_changed, request_media_type_removed,
  security_requirement_changed, response_header_removed, read_write_only_changed, deprecated
```

## SLO Definitions

- `radar-api – POST /v1/webhooks p95 latency < baseline from the Performance SLOs lane before Q-6 (MEASURE FIRST) over every CI run on main` (EPIC Q — Q-6 changes the DNS path)
- `radar-core – self-diff of every Spec Corpus spec = 0 Changes (100%) over every CI run on main`
- `radar-core – known-break recall on the Spec Corpus = 100% for rules in the active Ruleset over every CI run on main`
- `radar-api – POST /v1/gateway/kong p95 latency < baseline of POST /v1/gateway/logs (MEASURE FIRST) over every CI run on main`
- Existing Performance SLOs lane thresholds stay in force.

## Global rules for every task in Q & R

- Global DoD per Core §3. Branch → PR → green CI → merge; never push to `main` (`CLAUDE.md`).
- **Cargo:** serialized, `-j 2`, `cargo clean` when finished; tests never mutate process env; queries go through `q!`/`qs!`/`qa!`; every migration runs on SQLite and Postgres with a down-migration.
- **Forbidden on every task** (referenced below as "Global forbidden"): weakening or skipping an existing test; logging secrets, email addresses or Evidence values; skipping the Postgres lane; hard-coded credentials.
- **Tasks that end red** (Q-4-T1, Q-6-T1, R-1-T1, R-8-T1): new failing tests merge marked `#[ignore = "<green task id>"]` (Rust) or `it.skip` with the task id in the name (UI); the green task removes the marker, and its Verify includes a grep for the task id in the test files printing nothing. The "skipping an existing test" ban covers pre-existing tests only.
- **New radar-api tests** written during or after Q-9 go into `src/tests/<domain>.rs`, not `lib.rs`.
- **Flag policy:** corrections to existing safety guarantees (Q-4, Q-5-T1/T2, Q-6) ship without flags, following the 2026-10-06 remediation precedent — a flag that can turn a safety fix off is itself a risk. New user-facing behaviour gets a flag or a recorded exemption in its task.

---

# EPIC Q — Clean, safe baseline

- **Objective:** Get every open hardening commit onto `main` with a fully green, protected pipeline, close the three data-exposure findings, and validate demand with real teams.
- **Tracer Bullet?:** YES — Q-1 runs the whole pipeline (UI test → CI → merge to `main`).
- **Mode:** DELIVERY
- **Definition of Done:**
  - `main` is protected with the Q-2 required checks; PR #77 merged and every CI check green on `main`
  - N-10, N-11, N-12 closed with regression tests; Q-8 assertive E2E green on compose
  - Pilot go/no-go recorded in `docs/pilot-2026Q4.md`
- **Business Value:** removes the blockers to letting anyone outside the author run Radar. KPI: Pilot Teams onboarded (target 3).
- **SLO:** webhook latency SLO above.
- **ADRs:** none new.
- **Architecture Memory:** `DEVELOPMENT_PLAN.md` → Architecture Memory; update after Q-9 (module map).
- **Smoke Test Story:** Q-8.
- **Runbook Link:** `docs/runbook.md` — Q-6 adds an "SSRF block fired" entry.

## Q-1 · PR #77 merges green ⚡ TRACER BULLET

- **Persona:** As the maintainer I want PR #77's commits on `main` with every check green so that later work starts from a trusted baseline.
- **Business Value:** 3 · **Priority:** H — every other story branches from `main`.
- **Size:** S · **INVEST:** I✓ N✓ V✓ E✓ S✓ T✓ · **DoR:** READY
- **External Dependencies:** GitHub Actions; PR #77 (`quality/epic-o-and-n-tail`, 39 commits ahead of `origin/main` on 2026-10-23).
- **Test Data:** existing `RULE` fixture in `EvolutionRulesPage.test.tsx`.

```gherkin
Scenario: The accessibility test waits for the rules to load
  Given the rules fetch resolves after the audience callout renders
  When the test "names the icon-only dismiss, close and delete controls" runs
  Then it finds the button "Delete rule Allow optional headers"

Scenario: PR #77 merges
  Given all checks on PR #77 are green
  When the maintainer merges it
  Then main contains all the PR's commits and CI on main is green

Scenario: A check is still red
  Given any check fails on PR #77
  Then the PR is not merged and the failure is fixed on the branch
```

### Q-1-T1 · Restore local build headroom
- **Hat:** PREPARATORY · **Tier:** V · **Run:** HITL
- **Goal:** Make room for a full `cargo test --workspace` build (~18 GB in `target/`).
- **Size Budget:** no code.
- **TDD Execution Order:** n/a (environment task).
- **Deliverables:** either ≥ 20 GB free on C: (see the suite's disk-cleanup notes) or `CARGO_TARGET_DIR` pointed at a drive with room, recorded in `PICKUP.md`.
- **Verify:** `df -h /c` shows ≥ 20 GB free, or `cargo metadata --format-version 1 | jq -r .target_directory` prints the new location.
- **Do not touch:** repository files other than `PICKUP.md`.
- **Shortcuts:** Allowed: nothing to cut · Forbidden: deleting anything outside known cache locations without asking.
- **Pull Gate:** none (root).
- **Unblocks:** Q-4-T1, Q-5-T1, Q-6-T1, Q-9-T1, R-1-T1, R-9-T1 · **Confidence:** 3 · **TD Created:** none

### Q-1-T2 · Fix the EvolutionRulesPage accessibility test race
- **Hat:** FEATURE (test correctness) · **Tier:** V · **Run:** AFK
- **Goal:** Make the test wait for the rule row instead of reading it synchronously.
- **Size Budget:** < 20 lines.
- **TDD Execution Order:** (1) reproduce red locally by delaying the mocked rules fetch (2) change the row lookup to `await screen.findByRole(...)` (3) remove the delay.
- **Required Interfaces:** `EvolutionRulesPage` delete button `aria-label={\`Delete rule ${rule.name}\`}` (unchanged).
- **Abstraction Check:** sibling tests already use `findByRole` with a 10 s timeout; reuse that.
- **Deliverables:** updated accessibility test.
- **Verify:** `pnpm --filter radar-ui test -- -t "names the icon-only"` green; the `Node / pnpm` check on PR #77 green.
- **Do not touch:** `EvolutionRulesPage.tsx` (the label is correct; N-22 owns it).
- **Shortcuts:** Allowed: none needed · Forbidden: `test.skip`; widening the name matcher so it passes without the row; Global forbidden.
- **Pull Gate:** A-1 confirmed by the red reproduction.
- **Unblocks:** Q-1-T3 · **Confidence:** 3 · **TD Created:** none

### Q-1-T3 · Merge PR #77 and resync local `main`
- **Hat:** PREPARATORY · **Tier:** V · **Run:** HITL
- **Goal:** Merge PR #77 once all checks pass; then reset local `main` (37 unpushed commits on 2026-10-23, all contained in the PR) to `origin/main`.
- **Verify:** `gh pr view 77 --json state -q .state` prints `MERGED`; `gh run list --branch main --limit 1` shows success; after the reset, `git rev-list --count origin/main..main` prints `0`.
- **Do not touch:** Dependabot PRs (Q-7).
- **Shortcuts:** Allowed: nothing to cut · Forbidden: admin-merge over a red check; resetting local `main` before `git cherry origin/main main` shows only `-` lines (works for merge, squash or rebase merges) or `git diff main origin/main --stat` is empty.
- **Pull Gate:** Q-1-T2 green in CI.
- **Unblocks:** Q-2-T1, Q-3-T1, Q-4-T1, Q-5-T1, Q-6-T1, Q-7-T1, Q-10-T1, R-1-T1, R-9-T1 · **Confidence:** 3 · **TD Created:** none

## Q-2 · Branch protection on `main` (N-34)

- **Persona:** As the maintainer I want `main` to refuse unreviewed or red merges so that a hurried push can't bypass CI.
- **Business Value:** 2 · **Priority:** M — history shows direct pushes. · **Size:** S · **DoR:** READY

```gherkin
Scenario: Direct push is refused
  Given branch protection is on
  When someone pushes a commit straight to main
  Then GitHub rejects the push

Scenario: Red check blocks merge
  Given a PR with a failing "Rust" check
  Then the merge button is disabled
```

### Q-2-T1 · Protect `main`
- **Hat:** PREPARATORY · **Tier:** V · **Run:** HITL
- **Goal:** Require `Rust`, `Rust (Postgres)`, `Node / pnpm`, `Coverage`, `OpenAPI spec` and 1 review on `main`.
- **Verify:** `gh api repos/PolycarpusTack/APIRadar/branches/main/protection -q '.required_status_checks.contexts'` lists the five checks.
- **Do not touch:** workflow files.
- **Shortcuts:** Allowed: allow admins to bypass the review count while working solo (record it as TD-Q1) · Forbidden: leaving status checks optional.
- **Pull Gate:** Q-1-T3 merged (check names stable on `main`).
- **Unblocks:** Q-7-T1 · **Confidence:** 3 · **TD Created:** TD-Q1 if admin bypass is enabled.

## Q-3 · SPIKE: Validate with three Pilot Teams

- **Persona:** As the product owner I want three teams to run Radar on their own APIs so that I know whether the named Blast Radius is worth building on.
- **Business Value:** 3 · **Priority:** H — A-5 is the highest-impact assumption in the plan. · **Size:** M · **Timebox:** ends 2026-12-31 · **DoR:** READY
- **Deliverable:** `docs/pilot-2026Q4.md` — per team: their spec formats and OpenAPI version, Consumer languages, gateway, which Evidence source they would use, whether they need the desktop app, what they would pay for or replace; then a go/no-go.
- **Exit:** go → graduates the Not-yet-specified lines it answers; no-go → EPIC R stops after R-2 and the project returns to maintenance.

```gherkin
Scenario: A Pilot Team completes a trial
  Given a team has a Producer spec and at least one Consumer
  When they run radar check on a real PR
  Then the pilot note records whether the PR comment named the right Consumers

Scenario: Fewer than three teams by the timebox
  Given the timebox ends with fewer than three teams
  Then the note records the shortfall and the go/no-go is made on what exists
```

### Q-3-T1 · Run the pilot and write the note
- **Hat:** PREPARATORY · **Tier:** R · **Run:** HITL
- **Goal:** Recruit up to three teams, run the 5-minute demo on their specs, and record the answers above.
- **Verify:** `docs/pilot-2026Q4.md` exists with one section per team and a go/no-go line.
- **Do not touch:** code.
- **Shortcuts:** Allowed: internal Mediagenix teams count as Pilot Teams · Forbidden: putting another team's spec contents or people's names in the note.
- **Pull Gate:** Q-1-T3 (pilots run `main`).
- **Unblocks:** R-8-T1; Not-yet-specified lines · **Confidence:** 2 · **TD Created:** none

## Q-4 · Per-Organisation weekly Digest (N-11)

- **Persona:** As an Organisation admin I want my Digest to contain only my Organisation's Diffs so that no other Organisation's data reaches my inbox.
- **Business Value:** 3 · **Priority:** H — cross-org disclosure. · **Size:** M · **DoR:** READY for T1; T2 gated on A-7
- **Root cause (verified):** `aggregate_digest(pool, org_id)` already scopes by org; the leak is the scheduled `send_digest`, which calls `aggregate_digest(pool, "")` and mails the global `RADAR_DIGEST_RECIPIENTS`.
- **Data governance:** Digest content is confidential (Producer names, Breaking Changes). Recipient emails are PII: classification restricted; retained until removed by an admin or the Organisation is deleted (cascade); never logged; DPIA-lite: purpose = operational notification, lawful basis = legitimate interest of the Organisation, data minimised to the address only.
- **Idempotency:** claim-before-send. Row `(org_id, 'digest_sent_' || iso_week)` in the existing `settings` table (PK `(org_id, key)`, migration 035) via `INSERT … ON CONFLICT DO NOTHING`; send only if the insert affected a row. On send failure, delete the claim row and log `digest.failed{org_id}` so the next run retries. No race across replicas.

```gherkin
Scenario: Two Organisations get separate Digests
  Given org A and org B each have a Breaking Change this week and their own recipients
  When the weekly Digest runs
  Then org A's recipients get a Digest naming only org A's Producers
  And org B's recipients get a Digest naming only org B's Producers

Scenario: An Organisation with no recipients
  Given org C has activity but no Digest recipients
  When the Digest runs
  Then no email is sent for org C and nothing about org C appears in other Digests

Scenario: Re-run in the same week
  Given org A's Digest for this ISO week was claimed
  When the scheduler fires again in the same week, on any replica
  Then no second email is sent
```

### Q-4-T1 · Failing test on the scheduled path
- **Hat:** FEATURE · **Tier:** X · **Run:** AFK
- **Goal:** Introduce a pure seam `plan_digests(pool) -> Vec<DigestPlan { org_id, recipients, data }>` that the scheduler will call, and test it: seed two Organisations; assert no plan contains both orgs' Producer names and no plan has an empty `org_id`.
- **TDD Execution Order:** (1) test red — the first `plan_digests` wraps today's global behaviour unchanged.
- **Required Interfaces:** `aggregate_digest(pool, org_id) -> DigestData`; new `plan_digests` (this task's hand-off).
- **Verify:** `cargo test -p radar-api digest_plan_per_org` red.
- **Do not touch:** SMTP transport (`send_digest`'s mail call) — the seam sits before it.
- **Shortcuts:** Allowed: in-memory SQLite · Forbidden: asserting on counts only (assert on Producer names); Global forbidden.
- **Hand-Off Artifacts:** CONTRACT SNAPSHOT: `plan_digests` v1.
- **Pull Gate:** Q-1-T1, Q-1-T3.
- **Unblocks:** Q-4-T2 · **Confidence:** 3 · **TD Created:** none

### Q-4-T2 · Per-org recipients and per-org sending
- **Hat:** FEATURE · **Tier:** X · **Run:** AFK
- **Goal:** Add the `digest_recipient` table and Settings API per A-7; make `plan_digests` iterate Organisations with their own recipients; make the scheduler claim-then-send per plan; drop the empty-`org_id` global path.
- **Size Budget:** ~400 lines incl. tests (migration + CRUD + loop).
- **TDD Execution Order:** (1) T1 test plus failing tests for the no-recipients and re-run scenarios and recipient CRUD org scoping (2) simplest implementation (3) refactor.
- **Deliverables:** migration `digest_recipient` with down-migration; `GET/POST/DELETE /v1/settings/digest-recipients` (org-scoped) in `docs/openapi.yaml`; claim-before-send marker; HELP.md row.
- **Verify:** `cargo test -p radar-api digest` green, including T1 and all three scenarios; `OpenAPI spec` check green.
- **Do not touch:** `preview_digest` route contract (K-5); SMTP transport.
- **Shortcuts:** Allowed: sequential per-org sends (TD-Q3) · Forbidden: a global recipient fallback in multi-org deployments; Global forbidden.
- **Infrastructure Dependencies:** `digest_recipient` table; `settings` keys `digest_sent_*`.
- **Observability Points:** log `digest.sent{org_id, change_count, recipient_count}` — no addresses.
- **Feature Flag:** none (safety correction). The recipients Settings UI is not in scope (API only).
- **Rollback Notes:** revert the PR; run the down-migration; `settings` marker keys are inert.
- **Pull Gate:** Q-4-T1 red; **A-7 confirmed by Yannick**.
- **Unblocks:** Q-8-T1 · **Confidence:** 2 · **TD Created:** TD-Q3

## Q-5 · Share Tokens only on explicit share (N-12)

- **Persona:** As a Producer owner I want a public share link to exist only when I choose to share a Diff so that viewing a Diff never publishes it.
- **Business Value:** 3 · **Priority:** H — every first view of a Diff currently publishes it. · **Size:** M · **DoR:** READY
- **Root cause (verified):** `get_diff` (`diffs.rs`) mints a Share Token when absent; the UI has no share control today.
- **Idempotency:** key = diff id. Conflict strategy: `UPDATE diff SET share_token = ? WHERE id = ? AND share_token IS NULL`, then re-read and return whatever token is stored, so concurrent POSTs converge on one token.

```gherkin
Scenario: Reading a Diff creates no Share Token
  Given a Diff with no Share Token
  When an authenticated user calls GET /v1/diffs/:id
  Then the Diff still has no Share Token

Scenario: Explicit share
  When the user calls POST /v1/diffs/:id/share
  Then a Share Token is created, its /share/:token URL is returned, and an audit event is written
  And calling it again, or concurrently, returns the same token

Scenario: Shared view uses evolution-rule severities
  Given an evolution rule of the Diff's Organisation downgrades a Change to non_breaking_risky
  When the Diff is opened via /share/:token
  Then the Change shows non_breaking_risky, as in the org view

Scenario: Foreign Organisation
  When a user from another Organisation calls POST /v1/diffs/:id/share
  Then the response is 404 and no token is created
```

### Q-5-T1 · Stop minting on read; add the share action
- **Hat:** FEATURE · **Tier:** X · **Run:** AFK
- **Goal:** Remove token minting from `get_diff`; add `POST /v1/diffs/:id/share` (org-scoped, conditional update as above, audit event `diff.shared{diff_id, org_id}`); update `docs/openapi.yaml`.
- **TDD Execution Order:** (1) failing tests for scenarios 1, 2, 4 (2) move minting into the new handler (3) refactor.
- **Required Interfaces:** existing `diff.share_token` column; random token generation already in `diffs.rs`; audit writer (`audit.rs`).
- **Verify:** `cargo test -p radar-api share_token` green; `OpenAPI spec` check green.
- **Do not touch:** `/share/:token` read route (Q-5-T2).
- **Shortcuts:** Allowed: nothing to cut · Forbidden: deriving the token from the Diff id; Global forbidden.
- **Observability Points:** audit event above; no token value in logs.
- **Rollback Notes:** revert; existing tokens keep working because the column is unchanged.
- **Pull Gate:** Q-1-T1, Q-1-T3.
- **Unblocks:** Q-5-T2, Q-5-T3 · **Confidence:** 3 · **TD Created:** none

### Q-5-T2 · Shared view applies evolution-rule severities
- **Hat:** FEATURE · **Tier:** X · **Run:** AFK
- **Goal:** Run the server-side evolution-rule evaluator in the shared view, loading rules for the Diff's service Organisation (`s.org_id`) because there is no caller Organisation.
- **TDD Execution Order:** (1) failing test for scenario 3 (2) call the evaluator (3) refactor.
- **Abstraction Check:** second use of the evaluator call site — note it; extract an `effective_changes(diff, org_id)` helper only on a third caller.
- **Verify:** `cargo test -p radar-api shared_view_severity` green.
- **Do not touch:** evolution-rule evaluation logic (F+).
- **Shortcuts:** Allowed: nothing to cut · Forbidden: using any Organisation other than the service's; Global forbidden.
- **Pull Gate:** Q-5-T1 merged.
- **Unblocks:** Q-5-T3 · **Confidence:** 3 · **TD Created:** none

### Q-5-T3 · Add a Share action to DiffDetailPage
- **Hat:** FEATURE · **Tier:** V · **Run:** AFK
- **Goal:** Add a "Share" button to DiffDetailPage that calls the share action and copies the returned link (new control; no share UI exists today).
- **TDD Execution Order:** (1) RTL test: no link shown before click; link shown and copied after; error state on failure (2) implement.
- **Verify:** `pnpm --filter radar-ui test -- -t "DiffDetailPage share"` green.
- **Do not touch:** ShareDiffPage rendering.
- **Shortcuts:** Allowed: plain clipboard copy with select-text fallback · Forbidden: calling the share endpoint on page load; Global forbidden.
- **Feature Flag:** `ui.diff_share_button`, default ON — new user-facing control; flag lets an operator hide sharing entirely.
- **Pull Gate:** Q-5-T1 endpoint in `docs/openapi.yaml`; Q-5-T2 merged (shared view correct before it is promoted).
- **Unblocks:** Q-8-T1 · **Confidence:** 3 · **TD Created:** none

## Q-6 · SSRF: non-blocking DNS; block IPv4-mapped and remaining private ranges (N-10)

- **Persona:** As a self-hosting operator I want outbound fetches to refuse every private address and never stall the server on DNS so that webhooks, scans and CSV runs can't reach internal services or starve request handling.
- **Business Value:** 3 · **Priority:** H — SSRF bypass on three outbound features. · **Size:** M · **DoR:** READY
- **Already done (verified):** connect-time pinning — `ssrf_pinned_client` uses `resolve_to_addrs` for webhooks, scans, CSV, catalog and batch_compare (on `main` since 64ec5ea). Rebinding is a regression guard here, expected green.
- **Open (verified):** (a) `ssrf_validated_addrs` calls blocking `to_socket_addrs()` from async handlers; (b) `is_rfc1918_or_loopback` doesn't unwrap IPv4-mapped IPv6 (`::ffff:10.0.0.1` passes) and doesn't block `0.0.0.0/8` or `100.64.0.0/10`; (c) there is no resolver seam for tests.

```gherkin
Scenario: IPv4-mapped IPv6
  When a scheduled scan targets a host resolving to ::ffff:10.0.0.1
  Then the scan is refused as a private address

Scenario: Unspecified and carrier-grade NAT ranges
  When a webhook targets a host resolving to 0.0.0.0 or 100.64.0.1
  Then the webhook is refused

Scenario: Rebinding host (regression guard)
  Given a resolver that returns a public IP first and 127.0.0.1 second
  When a webhook delivery targets it
  Then the request goes to the public IP or is refused, never to 127.0.0.1

Scenario: DNS does not block the runtime
  When 200 CSV Runner rows resolve hosts concurrently
  Then resolution runs through the async resolver, not on worker threads
```

### Q-6-T1 · Resolver seam and failing tests
- **Hat:** FEATURE · **Tier:** R · **Run:** AFK
- **Goal:** Add an injectable resolver parameter to the SSRF validation path (default = system resolver) and write tests for scenarios 1, 2 and 4 for the webhook, scan, CSV, catalog and batch_compare callers, plus scenario 3 as an expected-green guard; add a `webhook_create_p95` case to `radar-api/tests/slo_guard.rs` and record its baseline on `main` before the seam change.
- **TDD Execution Order:** (1) seam with no behaviour change (2) tests: 1, 2, 4 red; 3 green.
- **Verify:** `cargo test -p radar-api -- ssrf_mapped_v6 ssrf_cgnat ssrf_async_resolve` red; `cargo test -p radar-api ssrf_rebinding` green.
- **Do not touch:** `ssrf_pinned_client` connect-time pinning (O-15).
- **Shortcuts:** Allowed: a fake resolver in tests · Forbidden: real DNS in tests; Global forbidden.
- **Pull Gate:** Q-1-T1, Q-1-T3.
- **Unblocks:** Q-6-T2 · **Confidence:** 2 · **TD Created:** none

### Q-6-T2 · Async resolver with the full private-range filter
- **Hat:** FEATURE · **Tier:** R · **Run:** AFK
- **Goal:** Move resolution to an async `reqwest::dns::Resolve` implementation that applies the filter (unwrap IPv4-mapped IPv6; add `0.0.0.0/8`, `100.64.0.0/10`); delete the blocking `to_socket_addrs()` path.
- **Size Budget:** ~300 lines incl. tests.
- **TDD Execution Order:** (2) simplest resolver passing T1 (3) refactor the three callers onto one constructor.
- **Abstraction Check:** five callers share `ssrf_pinned_client` (webhooks, scans, csv_runner, catalog, diffs batch_compare), and the sync predicate `is_ssrf_blocked` runs at validation time in webhooks, scans, csv_runner and catalog. All move to the async path; creation-time 422s stay (async predicate). Mind the `SSRF_BYPASS` test seam in csv_runner. If this exceeds ~500 LOC, split into T2a (async validator + predicate callers) and T2b (Resolve-based pinned client).
- **Verify:** `cargo test -p radar-api ssrf` green (T1 + existing SSRF tests); webhook latency SLO within baseline.
- **Do not touch:** webhook retry/backoff logic; CSV Runner retry policy.
- **Shortcuts:** Allowed: nothing to cut · Forbidden: `to_socket_addrs()` or any blocking resolve on the async runtime; Global forbidden.
- **Observability Points:** log `ssrf.blocked{caller, reason}` without the URL query string; runbook entry "SSRF block fired".
- **Rollback Notes:** revert the PR (no schema change).
- **Pull Gate:** Q-6-T1 red.
- **Unblocks:** Q-8-T1 · **Confidence:** 2 · **TD Created:** none

## Q-7 · Dependabot triage

- **Persona:** As the maintainer I want the low-risk dependency PRs merged and the risky ones listed so that the PR queue reflects real decisions.
- **Business Value:** 2 · **Priority:** M — ~20 open PRs hide which upgrades need work. · **Size:** S · **DoR:** READY

```gherkin
Scenario: Low-risk group merges
  Given a minor/patch or GitHub Actions group PR with green CI
  Then it is merged

Scenario: Superseded PR
  Given two group PRs bump the same packages
  Then the older one is closed with a link to the newer

Scenario: Major bump
  Given a PR that bumps a major version
  Then it stays open and is listed in the plan as its own upgrade story
```

### Q-7-T1 · Merge low-risk groups, close superseded, list majors
- **Hat:** PREPARATORY · **Tier:** V · **Run:** HITL
- **Goal:** Rebase and merge the open minor/patch and Actions group PRs one at a time; close superseded ones; add the open major bumps with PR numbers to Not yet specified.
- **Verify:** `gh pr list --author app/dependabot --state open` shows only major-version PRs, each named in Not yet specified.
- **Do not touch:** major-version PRs.
- **Shortcuts:** Allowed: nothing to cut · Forbidden: batch-merging majors; merging on red CI.
- **Pull Gate:** Q-1-T3; Q-2-T1 (merges go through protection).
- **Unblocks:** END OF STORY SEQUENCE · **Confidence:** 3 · **TD Created:** none

## Q-8 · SMOKE: Assertive E2E on docker-compose (N-33)

- **Persona:** As the maintainer I want an end-to-end test that fails when the core journey breaks so that a green CI means the product works.
- **Business Value:** 3 · **Priority:** H — the full-stack journey doesn't yet assert the Blast Radius or Share, and the mocked smoke suite skips its API check. · **Size:** M · **DoR:** READY
- **Test Data:** `fixtures/seed-demo.sh` + `fixtures/demo-payments-api/v1.yaml`, `v2.yaml`.

```gherkin
Scenario: Core journey
  Given the compose stack is up and seeded with the demo fixtures
  When radar check runs payments-api v1 → v2 against the API
  Then the verdict is BLOCKED with 1 Breaking Change
  And the Diff page lists billing-svc and mobile-gateway in the Blast Radius
  And the Share action produces a working /share/:token page

Scenario: Backend missing
  Given the API container is down
  Then the E2E job fails instead of skipping
```

### Q-8-T1 · Compose-backed Playwright job with real assertions
- **Hat:** FEATURE · **Tier:** X · **Run:** AFK
- **Goal:** Extend the existing `Full-stack E2E` job (`radar-ui/e2e-fullstack/journey.spec.ts`, already a real radar-api with `retries: 0`) with the `radar check` → Diff page → Blast Radius → Share assertions, moving it onto the compose stack if that is simpler; delete the API-health test guarded by `test.skip`/`isVisible()` in the mocked `e2e/smoke.spec.ts`.
- **TDD Execution Order:** (1) add the assertions; show them red with `billing-svc` removed from the seed (2) green (3) delete the tolerant mocked test.
- **Verify:** the `Full-stack E2E` job is green on a branch and red with `billing-svc` removed from the seed (both runs linked in the PR).
- **Do not touch:** the mocked `e2e/` suite beyond the deleted test.
- **Shortcuts:** Allowed: Chromium only (TD-Q4) · Forbidden: retries that hide flakiness (`retries: 0`); Global forbidden.
- **Pull Gate:** Q-4-T2, Q-5-T3, Q-6-T2 merged.
- **Unblocks:** Q-9-T1, R-8-T2 (extends this job), R-10-T1 · **Confidence:** 2 · **TD Created:** TD-Q4

## Q-9 · Decompose `radar-api/src/lib.rs` (N-27)

- **Persona:** As the maintainer I want `lib.rs` split and the flat modules grouped along the design's module map so that a change to one domain stays in one place.
- **Business Value:** 2 · **Priority:** M — `lib.rs` is 5,788 lines, ~5,000 of them a 155-test inline module; the flat 27-file layout has drifted from `SOLUTION_DESIGN §4.5`. · **Size:** L (7 tasks) · **DoR:** READY
- All tasks are REFACTORING: no behaviour change, no test body edited, test count recorded before and after each task and unchanged.
- **Shared Shortcuts for Q-9-T1..T5:** Allowed: scripted cut/paste of whole test functions (no hand-retyped code) · Forbidden: editing any moved test body; converting to integration tests that need new `pub` items; Global forbidden.

```gherkin
Scenario: Behaviour unchanged
  Given the test suite passes with N tests before a Q-9 task
  When the task lands
  Then exactly N tests pass and no test body changed
```

### Q-9-T1..T5 · Move the inline test module out, by domain (5 batches)
- **Hat:** REFACTORING · **Tier:** X · **Run:** AFK
- **Goal:** Move the `#[cfg(test)]` module from `lib.rs` into child modules under `src/tests/`, one batch per task, each ≤ ~1,000 moved lines: **T1** ingestion/gateway · **T2** diffs/blast-radius · **T3** evolution rules/policy · **T4** auth/Organisation · **T5** remainder (incl. shared test helpers, moved first if a batch needs them).
- **Size Budget:** ≤ ~1,000 moved lines per task; scripted move, so output tokens stay small.
- **TDD Execution Order:** n/a beyond "suite green before and after" (REFACTORING).
- **Verify (each):** `cargo test -p radar-api 2>&1 | grep "test result"` shows the same passed count as before the task (both numbers in the PR); `git diff --stat` shows the moved lines leaving `lib.rs`.
- **Do not touch:** non-test code; other batches' tests.
- **Shortcuts:** see shared Shortcuts above.
- **Pull Gate:** T1: Q-1-T1, Q-8-T1 green on `main`. T2..T5: the previous batch merged.
- **Unblocks:** T1→T2→T3→T4→T5→Q-9-T6 · **Confidence:** 3 · **TD Created:** none

### Q-9-T6 · Extract router and AppState
- **Hat:** REFACTORING · **Tier:** X · **Run:** AFK
- **Goal:** Move `build_router`, `AppState`, `run` and the middleware stack into their own modules; `lib.rs` keeps module declarations and re-exports.
- **Verify:** `cargo test --workspace` same count, green; `cargo clippy --all-targets -- -D warnings` clean; `wc -l radar-api/src/lib.rs` < 200.
- **Do not touch:** route paths, middleware order.
- **Shortcuts:** Allowed: scripted moves · Forbidden: reordering middleware or routes; Global forbidden.
- **Pull Gate:** Q-9-T5 merged.
- **Unblocks:** Q-9-T7 · **Confidence:** 3 · **TD Created:** none

### Q-9-T7 · Group flat modules by §4.5 domain and reconcile the doc
- **Hat:** REFACTORING · **Tier:** X · **Run:** AFK
- **Goal:** Group the flat `src/*.rs` modules into the `SOLUTION_DESIGN §4.5` domains (diffs, evidence, impact, policy, artifacts, catalog, authz, audit) where the fit is clear; update §4.5 and Architecture Memory to the real layout, recording any module left flat and why.
- **Verify:** every module named in §4.5 exists under `radar-api/src/` and every directory under `radar-api/src/` is named in §4.5 (checked with `ls -R` against the doc); suite green, same count.
- **Do not touch:** handler bodies (moves only); public API paths.
- **Shortcuts:** Allowed: leaving a module flat with a one-line reason in §4.5 · Forbidden: renaming public types; Global forbidden.
- **Pull Gate:** Q-9-T6 merged.
- **Unblocks:** END OF STORY SEQUENCE · **Confidence:** 2 · **TD Created:** none

## Q-10 · Docs and status tell the truth

- **Persona:** As a new reader I want the status, plan and pickup notes to agree so that I don't act on stale guidance.
- **Business Value:** 1 · **Priority:** L — misleads readers but blocks nothing. · **Size:** S · **DoR:** READY

```gherkin
Scenario: Status agrees everywhere
  Then the suite README cluster table, 21's README header and DEVELOPMENT_ORDER.md name the same status
```

### Q-10-T1 · Reconcile status, PICKUP and untracked files
- **Hat:** REFACTORING · **Tier:** V · **Run:** HITL
- **Goal:** Align 21's README status with the suite README; rewrite `PICKUP.md` for EPIC Q/R; Yannick decides commit-or-drop for `docs/cliff-notes.md` and `docs/APIRadar_Icon.png`.
- **Verify:** `git status --short docs/` prints nothing; `grep -n "Status:" README.md` matches the suite table.
- **Do not touch:** the OpenAPI version claim (owned by R-2-T2).
- **Shortcuts:** Allowed: nothing to cut · Forbidden: changing a status without the matching DEVELOPMENT_ORDER.md stage tick.
- **Pull Gate:** Q-1-T3.
- **Unblocks:** END OF STORY SEQUENCE · **Confidence:** 3 · **TD Created:** none

### EPIC Q — Phase Gate Checklist

- [ ] Q-1 PR #77 merged, `main` green, local `main` resynced
- [ ] Q-2 `main` protected
- [ ] Q-3 pilot note with go/no-go
- [ ] Q-4, Q-5, Q-6 closed with regression tests
- [ ] Q-7 only major-version Dependabot PRs open
- [ ] Q-8 assertive E2E green on `main`
- [ ] Q-9 `lib.rs` < 200 lines, module map reconciled
- [ ] Q-10 status consistent; PICKUP refreshed
- [ ] Phase Summary + Architecture Memory updated; EPIC retro (incl. environment)

---

# EPIC R — A credible diff, cheaper evidence

- **Objective:** Make "no Breaking Changes" trustworthy for OpenAPI 3.0 and 3.1, close the highest-value rule gaps, and let a gateway send usage Evidence without a forwarder.
- **Tracer Bullet?:** NO (R-2 is this EPIC's first end-to-end slice: a 3.1 spec through CLI → API → PR comment).
- **Mode:** DELIVERY
- **Definition of Done:**
  - A 3.1 spec produces correct Changes end to end; docs state the supported versions accurately
  - Ruleset `v2` passes the Spec Corpus lane (R-10) and is the default
  - Kong logs produce usage Evidence via `POST /v1/gateway/kong`, covered by the compose E2E job
- **Business Value:** removes the most likely reason a Pilot Team rejects Radar (their spec doesn't parse, or a break is missed). KPI: Spec Corpus known-break recall = 100% for the active Ruleset.
- **SLOs:** corpus and Kong SLOs above.
- **ADRs:** ADR-017, ADR-018.
- **Smoke Test Story:** R-10 (corpus through the `radar check` CLI, plus one pair end to end on the compose stack). R-8-T2 also extends the Q-8 compose job.
- **Runbook Link:** `docs/runbook.md` — R-4-T1 adds "Ruleset flip and rollback".

## R-1 · SPIKE: OpenAPI 3.1 parsing (ADR-017)

- **Persona:** As a Producer on FastAPI I want my 3.1 spec to diff correctly so that Radar works on the spec I actually have.
- **Business Value:** 3 · **Priority:** H — A-2 is high impact. · **Size:** S · **DoR:** READY
- **Deliverable:** failing tests + ADR-017 (Proposed) with a results table; Yannick accepts it.

```gherkin
Scenario: A 3.1 spec with type arrays
  Given a spec with `type: [string, "null"]` on a response field
  When radar check diffs it against itself
  Then it reports 0 Changes (today: expected to fail parsing — record the actual result)
```

### R-1-T1 · Failing 3.1 Spec Corpus tests
- **Hat:** FEATURE · **Tier:** X · **Run:** AFK
- **Goal:** Extend `fixtures/self-diff-corpus/` with 3.1 specs (a FastAPI-generated spec, plus hand-written cases: type arrays, `const`, `examples` arrays, `webhooks`, `$ref` siblings), each with a self-diff test and one known-break pair.
- **TDD Execution Order:** (1) tests only — this task ends red.
- **Verify:** `cargo test -p radar-core openapi_31` — the failures and their messages recorded in the PR.
- **Do not touch:** `diff_openapi` logic.
- **Shortcuts:** Allowed: hand-written minimal specs · Forbidden: fixtures copied from proprietary specs; Global forbidden.
- **Pull Gate:** Q-1-T1, Q-1-T3.
- **Unblocks:** R-1-T2 · **Confidence:** 3 · **TD Created:** none

### R-1-T2 · Compare approaches and propose ADR-017
- **Hat:** PREPARATORY · **Tier:** R · **Run:** HITL (Yannick accepts the ADR)
- **Goal:** Prototype on a throwaway branch: (a) `oas3` crate, (b) normalise 3.1 → 3.0 before `openapiv3`, (c) oasdiff for OpenAPI (feeds R-3). Record pass counts on R-1-T1 tests, estimated LOC touched in `diff.rs` (~1,500 non-test lines, 15 `openapiv3::` uses today), and what each approach loses.
- **Verify:** ADR-017 exists as Proposed with a results table per approach and, if the chosen approach exceeds ~500 LOC, a split proposal for R-2-T1; then Yannick marks it Accepted.
- **Do not touch:** `main` code (prototype branch only).
- **Shortcuts:** Allowed: throwaway prototype code · Forbidden: merging prototype code.
- **Pull Gate:** R-1-T1 red results recorded.
- **Unblocks:** R-2-T1, R-3-T1 · **Confidence:** 2 · **TD Created:** none

## R-2 · OpenAPI 3.1 end to end

- **Persona:** As a Producer on 3.1 I want `radar check` and the PR comment to report my Breaking Changes so that the PR gate protects me too.
- **Business Value:** 3 · **Priority:** H — without it a 3.1 Pilot Team can't use Radar at all. · **Size:** M · **DoR:** HOLD until ADR-017 is Accepted

```gherkin
Scenario: 3.1 Breaking Change in a PR
  Given a 3.1 spec where head removes a required response field
  When radar-action runs on the PR
  Then the PR comment shows 1 Breaking Change with the Blast Radius

Scenario: 3.0 behaviour unchanged
  Given the existing 3.0 test suite
  Then every 3.0 test passes unchanged

Scenario: Unsupported construct
  Given a 3.1 construct the chosen approach can't model
  Then radar check exits with a parse error naming the construct, not a silent skip
```

### R-2-T1 · Implement ADR-017 in `radar-core`
- **Hat:** FEATURE · **Tier:** R · **Run:** AFK
- **Goal:** Make the R-1-T1 tests pass using the ADR-017 approach (split per the ADR's proposal if it exceeds ~500 LOC).
- **Size Budget:** ≤ ~500 LOC per task; ceiling 1,500.
- **TDD Execution Order:** (1) R-1-T1 tests are the red set (2) simplest passing implementation (3) refactor.
- **Verify:** `cargo test -p radar-core openapi` green (3.0 and 3.1).
- **Do not touch:** GraphQL and protobuf diff modules.
- **Shortcuts:** Allowed: deferring a construct the ADR names as deferred (TD Item) · Forbidden: dropping unknown keywords silently; Global forbidden.
- **Pull Gate:** ADR-017 Accepted.
- **Unblocks:** R-2-T2 · **Confidence:** 2 · **TD Created:** TD for any construct ADR-017 defers.

### R-2-T2 · Action fixture + docs truth
- **Hat:** FEATURE · **Tier:** V · **Run:** AFK
- **Goal:** Add a `radar-action (OpenAPI 3.1 — field removed)` CI case; update README, HELP.md and `docs/cliff-notes.md` (if kept) to the verified version support.
- **TDD Execution Order:** (1) action case red on the pre-R-2 binary (2) green on the new one.
- **Verify:** the new action CI job green; `grep -rn "3\.x\|3\.0\|3\.1" README.md HELP.md` shows only accurate statements.
- **Do not touch:** existing action cases.
- **Shortcuts:** Allowed: one fixture pair · Forbidden: claiming versions the corpus doesn't cover.
- **Pull Gate:** R-2-T1 merged.
- **Unblocks:** R-10-T1 · **Confidence:** 3 · **TD Created:** none

## R-3 · SPIKE: Rule gap matrix against oasdiff (ADR-018)

- **Persona:** As the maintainer I want to know which breaking-change rules Radar misses so that I fix the ones that matter instead of guessing.
- **Business Value:** 3 · **Priority:** H — decides whether R-4..R-7 are rule ports or output mappings. · **Size:** M · **DoR:** READY
- **Deliverable:** `docs/diff-rule-gap-matrix.md` (rule, oasdiff id, Radar result, frequency in the corpus) and ADR-018 (Proposed → Accepted by Yannick).

### R-3-T1 · Extend the Spec Corpus with real pairs and run both engines
- **Hat:** PREPARATORY · **Tier:** R · **Run:** AFK
- **Goal:** Add ≥ 20 public spec pairs (from the git history of public OpenAPI repos with clear licences) to the Spec Corpus with expected-Changes files; run `radar check` and `oasdiff breaking` on each; tabulate differences.
- **Verify:** `docs/diff-rule-gap-matrix.md` lists every pair with both engines' break counts and the rule ids that differ.
- **Do not touch:** `radar-core` code.
- **Shortcuts:** Allowed: a comparison script under `scripts_tmp/` · Forbidden: specs with unclear licences.
- **Pull Gate:** R-1-T2 (3.1 parsing decision known).
- **Unblocks:** R-3-T2, R-10-T1 · **Confidence:** 2 · **TD Created:** none

### R-3-T2 · Propose ADR-018
- **Hat:** PREPARATORY · **Tier:** R · **Run:** HITL (Yannick accepts the ADR)
- **Goal:** Decide port vs optional oasdiff Diff Engine, weighing binary size, licence (Apache-2.0), desktop bundling, and how Blast Radius matching consumes Change paths; fix the final ChangeKind v2 names.
- **Verify:** ADR-018 Proposed with the decision table, then Accepted; the ChangeKind Contract Snapshot updated with final names; R-4..R-7 Goals updated if the decision is "oasdiff engine".
- **Do not touch:** `radar-core` code.
- **Shortcuts:** Allowed: nothing to cut · Forbidden: deciding without the matrix.
- **Pull Gate:** R-3-T1 matrix complete.
- **Unblocks:** R-4-T1 · **Confidence:** 2 · **TD Created:** none

## R-4 · Request-side rules + Ruleset flag

- **Persona:** As a Consumer team I want a changed parameter type or location, or a removed request media type, flagged as breaking so that my calls don't start failing after deploy.
- **Business Value:** 3 · **Priority:** H — most common real-world break class after field removal (to be confirmed by the R-3 matrix). · **Size:** M · **DoR:** HOLD until ADR-018

```gherkin
Scenario: Parameter type narrowed
  Given query param `limit` changes from string to integer
  Then a Breaking Change is reported under Ruleset v2
  And no Change is reported under Ruleset v1

Scenario: Parameter moved from query to header
  Then a Breaking Change is reported under v2

Scenario: Request media type removed
  Given the request body drops application/xml
  Then a Breaking Change is reported under v2
```

### R-4-T1 · Introduce `RADAR_DIFF_RULESET` (default v1)
- **Hat:** PREPARATORY · **Tier:** X · **Run:** AFK
- **Goal:** Thread a `Ruleset` value through `diff_openapi` and the CLI/API config; v1 = today's behaviour; add the runbook entry "Ruleset flip and rollback".
- **TDD Execution Order:** (1) failing `ruleset_default_is_v1` and `ruleset_parse_selects_v2` tests on a pure `Ruleset::parse(Option<&str>)` (2) read the env once in CLI/API bootstrap and thread the value (3) refactor.
- **Verify:** `cargo test -p radar-core ruleset_default_is_v1` green; `cargo test --workspace` green with no expectation changes.
- **Do not touch:** change classification logic.
- **Shortcuts:** Allowed: parse the env var once at startup · Forbidden: changing any v1 expectation; Global forbidden.
- **Feature Flag:** `RADAR_DIFF_RULESET` default `v1`.
- **Pull Gate:** ADR-018 Accepted.
- **Unblocks:** R-4-T2, R-5-T1, R-6-T1, R-7-T1 · **Confidence:** 3 · **TD Created:** TD-R1

### R-4-T2 · Request-side rules under v2
- **Hat:** FEATURE · **Tier:** X · **Run:** AFK
- **Goal:** Implement the three scenarios under v2 (or map oasdiff's ids, per ADR-018), adding their ChangeKinds across every Contract Snapshot touchpoint.
- **TDD Execution Order:** (1) failing tests for the three scenarios (2) simplest passing rules (3) refactor.
- **Required Interfaces:** CONTRACT SNAPSHOT: ChangeKind v2 additions.
- **Verify:** `cargo test -p radar-core ruleset_v2_request` green; `OpenAPI spec` check green; `pnpm --filter radar-ui typecheck` clean.
- **Do not touch:** response-side diff.
- **Shortcuts:** Allowed: nothing to cut · Forbidden: changing v1 expectations; adding a kind without its touchpoints; Global forbidden.
- **Pull Gate:** R-4-T1 merged.
- **Unblocks:** R-10-T1 · **Confidence:** 2 · **TD Created:** none

## R-5 · Security requirement rules

- **Persona:** As a Consumer team I want a new auth scheme or scope requirement flagged so that I don't start getting 401s.
- **Business Value:** 2 · **Priority:** M — less frequent than R-4, but the break is total. · **Size:** S · **DoR:** HOLD until ADR-018

```gherkin
Scenario: Scope added
  Given an operation's security requirement adds scope `orders:write`
  Then a Breaking Change is reported under v2

Scenario: Security removed
  Given an operation drops its security requirement
  Then a non_breaking_risky Change is reported under v2
```

### R-5-T1 · Security diff under v2
- **Hat:** FEATURE · **Tier:** X · **Run:** AFK
- **Goal:** Compare effective security (global + operation override) per operation; add the ChangeKind across its touchpoints.
- **TDD Execution Order:** (1) failing tests for both scenarios (2) simplest passing rule (3) refactor.
- **Required Interfaces:** CONTRACT SNAPSHOT: ChangeKind v2 additions.
- **Verify:** `cargo test -p radar-core ruleset_v2_security` green; `OpenAPI spec` check green.
- **Do not touch:** OIDC/auth code in `radar-api` (unrelated "security").
- **Shortcuts:** Allowed: treat any scheme change as breaking (no scheme-equivalence logic) · Forbidden: changing v1 expectations; Global forbidden.
- **Pull Gate:** R-4-T1 merged.
- **Unblocks:** R-10-T1 · **Confidence:** 2 · **TD Created:** none

## R-6 · Response headers and readOnly/writeOnly

- **Persona:** As a Consumer team I want removed response headers and readOnly/writeOnly flips flagged so that pagination links and write payloads keep working.
- **Business Value:** 2 · **Priority:** M — silent breaks that field-level rules miss. · **Size:** M · **DoR:** HOLD until ADR-018

```gherkin
Scenario: Response header removed
  Given the 200 response drops header `Link`
  Then a Breaking Change is reported under v2

Scenario: Field becomes readOnly
  Given request field `status` becomes readOnly
  Then a Breaking Change is reported for the request under v2
```

### R-6-T1 · Header and readOnly/writeOnly rules under v2
- **Hat:** FEATURE · **Tier:** X · **Run:** AFK
- **Goal:** Implement both rule families with Change paths in the existing `impact_evidence.field_path` format; add their ChangeKinds across the touchpoints.
- **TDD Execution Order:** (1) failing tests for both scenarios and one Blast Radius match on a header path (2) simplest passing rules (3) refactor.
- **Required Interfaces:** CONTRACT SNAPSHOT: ChangeKind v2 additions; `impact_evidence.field_path` format.
- **Verify:** `cargo test -p radar-core -- ruleset_v2_headers ruleset_v2_read_write_only` green; `OpenAPI spec` check green.
- **Do not touch:** Evidence matching code.
- **Shortcuts:** Allowed: nothing to cut · Forbidden: a new path format; changing v1 expectations; Global forbidden.
- **Pull Gate:** R-4-T1 merged.
- **Unblocks:** R-10-T1 · **Confidence:** 2 · **TD Created:** none

## R-7 · Deprecation Signals with named Consumers

- **Persona:** As a Producer owner I want the PR comment to list which Consumers still use what I'm deprecating so that I know whom to warn before removing it.
- **Business Value:** 3 · **Priority:** M — turns the differentiator into a migration tool; never blocks, so lower urgency. · **Size:** M · **DoR:** HOLD until ADR-018

```gherkin
Scenario: Operation deprecated
  Given head marks GET /users/{id} as deprecated
  And billing-svc has runtime Evidence on it in the last 30 days
  Then the PR comment shows a non_breaking_risky Change naming billing-svc

Scenario: Deprecated with no users
  Given no Evidence exists for the deprecated operation
  Then the Change is shown with "no known Consumers"
```

### R-7-T1 · Detect Deprecation Signals
- **Hat:** FEATURE · **Tier:** X · **Run:** AFK
- **Goal:** Report `deprecated: true` on operations, parameters and schema properties, and new `Sunset` response headers, as non_breaking_risky under v2; add the ChangeKind across its touchpoints.
- **TDD Execution Order:** (1) failing tests per location (2) simplest passing detection (3) refactor.
- **Required Interfaces:** CONTRACT SNAPSHOT: ChangeKind v2 additions.
- **Verify:** `cargo test -p radar-core ruleset_v2_deprecation` green; `OpenAPI spec` check green.
- **Do not touch:** Blast Radius matching (R-7-T2).
- **Shortcuts:** Allowed: ignore `x-deprecated` vendor extensions · Forbidden: severity above non_breaking_risky; Global forbidden.
- **Pull Gate:** R-4-T1 merged.
- **Unblocks:** R-7-T2 · **Confidence:** 3 · **TD Created:** none

### R-7-T2 · Blast Radius and PR comment for deprecations
- **Hat:** FEATURE · **Tier:** X · **Run:** AFK
- **Goal:** Include deprecation Changes in Blast Radius matching and render a "Still used by" column in the PR comment.
- **TDD Execution Order:** (1) failing `deprecation_blast_radius` and `pr_comment_still_used_by` tests (2) implement (3) refactor.
- **Verify:** `cargo test -p radar-api -- deprecation_blast_radius` and `cargo test -p radar-cli -- pr_comment_still_used_by` green (the PR comment is built in `radar-cli` `github.rs::build_comment_with_suites`).
- **Do not touch:** policy engine verdicts.
- **Shortcuts:** Allowed: reuse the existing evidence table renderer · Forbidden: letting deprecations affect policy verdicts; Global forbidden.
- **Pull Gate:** R-7-T1 merged.
- **Unblocks:** R-10-T1 · **Confidence:** 2 · **TD Created:** none

## R-8 · Kong Gateway Adapter

- **Persona:** As a platform engineer running Kong I want Kong to send its logs to Radar directly so that every Consumer behind the gateway shows up as Evidence without code changes.
- **Business Value:** 3 · **Priority:** M — removes per-service setup, but depends on the pilot's gateway answer. · **Size:** M · **DoR:** HOLD until R-8-T1 records its three decisions
- **External Dependencies:** Kong 3.x `http-log` plugin payload (recorded and scrubbed fixture).
- **Verified constraint:** `ingest_gateway_logs` has no dedup (rows get `Uuid::new_v4()`, `recorded_at = now`) and requires existing Radar `consumer_id`/`service_id` (`require_evidence_owned`). The adapter therefore needs its own key and mappings.
- **Data governance:** keep only method, matched route, Consumer, status and Kong's timestamp; drop bodies, headers, client IP and query string before storage. Classification: internal.
- **Idempotency:** decided in R-8-T1 — proposed v5 UUID of (Kong request id or `started_at`, Kong consumer, route), `ON CONFLICT DO NOTHING`; `recorded_at` from Kong's timestamp.

```gherkin
Scenario: Kong log becomes Evidence
  Given a mapping from Kong consumer "billing-svc" to Radar Consumer billing-svc
  And a Kong http-log entry for that consumer on route "/orders/{id}" GET
  When it is POSTed to /v1/gateway/kong
  Then a runtime usage Evidence row exists for billing-svc on GET /orders/{id}

Scenario: Unmapped Kong consumer
  Given the entry's consumer has no mapping
  Then the entry is counted in gateway_kong_entries_total{outcome="unmapped"} and no Evidence or Consumer is created

Scenario: Retry
  When Kong re-sends the same batch
  Then no duplicate Evidence is written

Scenario: Foreign service
  Given the token's Organisation does not own the mapped service
  Then the whole batch is rejected with 403 before any write
```

### R-8-T1 · Record, scrub, decide, and write failing contract tests
- **Hat:** FEATURE · **Tier:** X · **Run:** HITL (recording needs a Kong instance)
- **Goal:** Record a Kong 3.x http-log batch; scrub it; record three decisions in the story (idempotency key; Kong consumer → Radar Consumer mapping, e.g. by Consumer name or an explicit mapping table; Kong service/route → Radar service mapping); write failing tests for the four scenarios.
- **Verify:** `cargo test -p radar-api gateway_kong` red; the three decisions written into this story; the fixture contains no IPs, credentials or real consumer names (`grep -E "[0-9]+\.[0-9]+\.[0-9]+\.[0-9]+|apikey|Authorization" fixtures/kong/*` prints nothing).
- **Do not touch:** `/v1/gateway/logs` request contract.
- **Shortcuts:** Allowed: a local Kong in Docker for recording · Forbidden: a hand-invented payload; committing the recording unscrubbed; Global forbidden.
- **Pull Gate:** Q-3 gateway answer (or Yannick confirms Kong).
- **Unblocks:** R-8-T2 · **Confidence:** 2 · **TD Created:** none

### R-8-T2 · Implement `POST /v1/gateway/kong`
- **Hat:** FEATURE · **Tier:** X · **Run:** AFK
- **Goal:** Implement the adapter per R-8-T1's decisions, extending the gateway Evidence writer with the dedup key and source timestamp; document the Kong plugin config in `docs/runtime-usage-ingestion.md` and HELP.md; add to `docs/openapi.yaml`; add a Kong ingest → Blast Radius step to the Q-8 compose job.
- **TDD Execution Order:** (2) simplest passing implementation of the T1 tests (3) refactor.
- **Abstraction Check:** extend the existing gateway writer (keyed insert) rather than writing a second one.
- **Verify:** `cargo test -p radar-api gateway_kong` green; `OpenAPI spec` check green; compose E2E job green with the Kong step.
- **Do not touch:** `/v1/gateway/logs` request contract (its rows may gain the optional key column, nothing else).
- **Shortcuts:** Allowed: a mapping table managed by API only (no UI) · Forbidden: auto-creating Consumers from Kong data; Global forbidden.
- **Observability Points:** metric `gateway_kong_entries_total{outcome}` (accepted, duplicate, unmapped, rejected).
- **Feature Flag:** none — opt-in by configuring Kong; recorded exemption.
- **Rollback Notes:** revert; run the down-migration for any key column or mapping table.
- **Pull Gate:** R-8-T1 red; Q-8-T1 job exists.
- **Unblocks:** END OF STORY SEQUENCE · **Confidence:** 2 · **TD Created:** none

## R-9 · Scanner stops inventing paths

- **Persona:** As a Producer owner I want static Evidence to name only operations that exist so that the Blast Radius doesn't list Consumers of `/categorys`.
- **Business Value:** 2 · **Priority:** M — false Consumers erode trust in the Blast Radius. · **Size:** S · **DoR:** READY
- **Flag:** none — recorded exemption: it only lowers confidence or corrects paths of static Evidence, which never blocks on its own at low confidence.

```gherkin
Scenario: Inferred path not in the spec
  Given generated-client method getCategory infers GET /categorys
  And the Producer spec has GET /categories
  Then the call site maps to GET /categories, or is recorded at low confidence

Scenario: Literal URL
  Given fetch("/users/1")
  Then the call site keeps medium confidence on GET /users/{id}
```

### R-9-T1 · Validate inferred operations against the spec
- **Hat:** FEATURE · **Tier:** X · **Run:** AFK
- **Goal:** When an operation comes from method-name inference, match it against the Producer's operations (with an irregular-plural table); unmatched inferences drop to low confidence.
- **TDD Execution Order:** (1) failing tests for both scenarios (2) simplest matcher (3) refactor.
- **Verify:** `cargo test -p radar-scanner inferred_operation` green.
- **Do not touch:** direct HTTP-client detection (N-17).
- **Shortcuts:** Allowed: a small fixed irregular-plural table · Forbidden: adding an inflection crate for this; Global forbidden.
- **Pull Gate:** Q-1-T1, Q-1-T3.
- **Unblocks:** END OF STORY SEQUENCE · **Confidence:** 2 · **TD Created:** none

## R-10 · SMOKE: Spec Corpus lane end to end; flip to Ruleset v2

- **Persona:** As the maintainer I want every PR to run the diff engine against the Spec Corpus through the real CLI and stack so that a rule regression can't merge.
- **Business Value:** 3 · **Priority:** H — the gate that makes v2 safe to default. · **Size:** M · **DoR:** HOLD until R-3-T1 corpus exists

```gherkin
Scenario: Self-diff
  Then every corpus spec diffed against itself with radar check gives 0 Changes

Scenario: Known breaks
  Then every expected Breaking Change in the corpus is reported under the active Ruleset

Scenario: End to end
  Given the compose stack
  When one 3.1 corpus pair is posted with radar check --api-url
  Then the Diff page shows the expected Breaking Change and Blast Radius

Scenario: Regression
  Given a commit that disables the readOnly rule
  Then the corpus lane fails
```

### R-10-T1 · Corpus lane through the CLI and compose stack
- **Hat:** FEATURE · **Tier:** X · **Run:** AFK
- **Goal:** Add a CI job that runs the built `radar check` binary over the Spec Corpus under both Rulesets and compares against expected-Changes files, plus one pair end to end on the Q-8 compose stack.
- **TDD Execution Order:** (1) remove one expectation → job red (2) restore → green.
- **Verify:** the job is green on a branch and red with one expectation removed (both runs linked in the PR).
- **Do not touch:** rule implementations (R-4..R-7).
- **Shortcuts:** Allowed: reuse the Q-8 compose job setup · Forbidden: marking corpus mismatches as allowed failures; Global forbidden.
- **Pull Gate:** Q-8-T1, R-2-T2, R-3-T1, R-4-T2, R-5-T1, R-6-T1, R-7-T2 merged.
- **Unblocks:** R-10-T2 · **Confidence:** 2 · **TD Created:** none

### R-10-T2 · Make v2 the default Ruleset
- **Hat:** FEATURE · **Tier:** V · **Run:** HITL
- **Goal:** Change the default to `v2`; release-note it as a behaviour change with the rollback (`RADAR_DIFF_RULESET=v1`).
- **TDD Execution Order:** (1) commit a snapshot of today's demo output (`radar check` on payments-api v1 → v2) under v1 (2) flip the default (3) compare.
- **Verify:** demo output under v2 matches the committed snapshot (or every difference is a v2 rule, listed in the PR); corpus lane green with default settings.
- **Do not touch:** the v1 code path (removed later, TD-R1).
- **Shortcuts:** Allowed: nothing to cut · Forbidden: removing v1 in the same release.
- **Rollback Notes:** set `RADAR_DIFF_RULESET=v1`; revert the default in a patch release.
- **Pull Gate:** R-10-T1 green on `main` for 5 runs.
- **Unblocks:** END OF STORY SEQUENCE · **Confidence:** 2 · **TD Created:** TD-R1 servicing date set to one release later.

### EPIC R — Phase Gate Checklist

- [ ] ADR-017 and ADR-018 Accepted
- [ ] OpenAPI 3.1 works end to end; docs state supported versions accurately
- [ ] Ruleset v2 default, corpus lane green
- [ ] Kong adapter documented, tested and in the compose E2E job
- [ ] Scanner no longer invents paths
- [ ] Phase Summary + Architecture Memory updated; EPIC retro (incl. environment)

---

## Technical Debt Items

| ID | Component | Problem | Principal | Interest | Servicing |
|---|---|---|---|---|---|
| TD-Q1 | GitHub settings | Admin bypass on review count (if enabled in Q-2) | one setting | unreviewed merges possible | when a second contributor joins |
| TD-Q3 | radar-api notifications | Digests sent sequentially per Organisation | bounded concurrency in the send loop | Digest run time grows linearly with Organisations | when a deployment exceeds 50 Organisations |
| TD-Q4 | CI | E2E runs Chromium only | add Firefox/WebKit projects | browser-specific UI bugs ship unseen | if Q-3 reports a non-Chromium user base |
| TD-R1 | radar-core | Ruleset `v1` code path kept for rollback | remove flag + v1 branch | two classification paths to maintain | one release after R-10-T2 |

## Not yet specified

- **Major dependency upgrades** (axum 0.8, sqlx 0.9, jsonwebtoken 11, thiserror 2, tower-http 0.6, React Router 7, Vite 8, Tailwind 4, TypeScript 7, ESLint 10, Electron 44): one upgrade story each; kept here because eleven stories would breach the 25-story limit. Graduates when Q-7-T1 lists the PRs still open.
- **Java, C# and Ruby scanning** (N-17 tail): which languages? Graduates from Q-3's Consumer-language answers.
- **Field-level scanner precision** (track the response variable instead of any same-named property in scope): how much data flow is enough? Graduates from a SPIKE after R-9.
- **Desktop auto-update and installer signing** (N-21, N-35): is there demand for the desktop app beyond the author? Graduates from Q-3.
- **GraphQL positioning** (stop leading with it; Hive and Cosmo own it): graduates from Q-3.
- **Envoy and Apigee Gateway Adapters**: graduates from Q-3 answers and the R-8 retro.
- **oasdiff as the default OpenAPI engine** (if ADR-018 chooses the optional engine): graduates from R-10 results.
- **Digest recipients Settings UI**: graduates if Pilot Teams use multi-org deployments (Q-3).

---

## Validation history

| Pass | Date | Result | Notes |
|---|---|---|---|
| Critic 1 | 2026-10-23 | REJECTED (1 BLOCK) | BLOCK: Q-9-T1 over the size ceiling → split into T1..T5 scripted batches. Premises the repo contradicted: Q-6 pinning already exists (re-scoped to async DNS + mapped IPv6 + CGNAT); Q-4 leak is `send_digest` and no per-org recipients exist (seam `plan_digests`, A-7, claim-before-send); Q-9-T3 had little to move (re-scoped to grouping flat modules); Q-5-T3 is a new control (flag added); R-8 gateway path has no dedup or mapping (decisions moved into R-8-T1, HOLD). Also added: ChangeKind Contract Snapshot + risk, missing Shortcuts/Do not touch/TDD order, DAG edge reconciliation, EPIC Q SLO, SLO windows, end-to-end R-10, Q-5 audit + conflict strategy, PII governance for recipients and Kong fixture, TD-Q3/Q4, risk review dates, Q-3 timebox, priority justifications. |
| Critic 2 | 2026-10-23 | APPROVED (0 BLOCK) | Applied now: Q-1-T3 cherry/tree-diff guard; `diff` table name; cargo `--` filter syntax; R-7-T2 Verify crate; Q-4 claim row keyed `(org_id, key)` with failure release; R-4-T1 pure `Ruleset::parse` test; Q-8 re-scoped onto the existing `Full-stack E2E` job (TD-Q2 dropped); Global rule for tasks that end red; Q-6 five callers + `is_ssrf_blocked` sites + `webhook_create_p95` baseline. **Carried follow-ups (fix before pulling the named task):** ChangeKind drift test in R-4-T2 + typecheck/SDK tests in R-5..R-7 Verify · Spec Corpus pair format snapshot (R-1-T1) · R-2-T1 unsupported-construct test · R-8-T2 split into T2a/b/c with listed migrations, `gateway_kong_p95` SLO case, mapping-table conflict strategy, runbook + retention note · Q-4-T2 recipients CRUD conflict strategy + CHANGELOG deliverable · R-3 Gherkin + pinned oasdiff version + R-3-T3 adapter contract tests if oasdiff engine · Q-9 batch-size waiver or 10 batches; Q-9-T7 path-only edits + `src/tests/` in §4.5 · TD-R2 (security-scheme equivalence), Kong mapping UI line · TD table Origin/Type/Cause/Compounding columns · Size Budget, INVEST and Test Data on remaining tasks/stories · ADR-019 (org-scoped REST conventions), ADR-020 (spec parse error taxonomy), ADR-021 (per-org Digest recipients, after A-7) · INFO items (Q-2 review-count check, R-7 `lookback_days`, R-6/R-9/Q-10 error flows, unsourced thresholds marked as judgement). |

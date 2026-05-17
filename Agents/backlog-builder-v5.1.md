# Backlog Builder v5.1

> **Depends on:** `core-specification-v1.md` for shared principles, execution modes, DoD, compression, and economics.
> **Executed by:** `backlog-builder-agent.md`

---

## 1. Mission

Transform a solution design into a sequential, dependency-aware, self-contained backlog for an AI code agent executing one task at a time. The backlog is a prioritised queue — not a fixed plan.

**Declare the execution mode (Core §1) before generating.** Mode determines which governance mechanisms are active.

---

## 2. Runtime Behaviour

- Ask questions only when missing information blocks decomposition.
- Make bounded assumptions, record them, proceed. (Core §5.3: if the spec is longer than the code, you've over-decomposed.)
- Keep each task self-contained for the code agent's context window. Target ≤ 3,000 tokens of project context per task (Core §4.4).
- Run the validator before returning.

**Output order:** Readiness Decision → Critical Gaps → Domain Glossary → Assumptions Ledger → Backlog → Validator Summary.

---

## 3. Rules (unique to backlog generation)

These ADD to the Core Specification principles (Core §2). They do not replace them.

1. **Sequential Pull System:** Tasks execute in strict order. Each task has an **Unblocks** field and a **Pull Gate** (Core §2 P8).

2. **Tracer Bullet First:** EPIC 1 is always a thin end-to-end slice through all layers. (Core §2, applied as: EPIC 1 = tracer bullet; subsequent EPICs add depth.)

3. **Token Budget:** ≤ 15,000 tokens per TASK output. Split proactively if > ~1,500 LOC or > 3 complex modules (stateful service, auth layer, data pipeline, external integration).

4. **Maximum Initial Depth:** ≤ 2 EPICs or 25 stories (whichever first). Expand after retro.

5. **Observability by Default:** Structured logging, golden signal metrics, distributed tracing, SLO definitions (`Service – Metric < Threshold over Window`), runbook per EPIC.

6. **Delivery Safety:** Schema changes include migration + rollback + feature flag. User-facing changes behind feature flags.

7. **Idempotency:** All write paths define idempotency keys and conflict strategies.

8. **Data Governance:** Classify sensitivity. Map retention. Anonymised test data when PII in scope.

9. **Reasonable Assumptions:** REST, UUIDs, PostgreSQL if unspecified, ISO-8601, UTC. **Unreasonable:** inventing auth flows, picking cloud vendors, redefining business rules, setting pricing.

---

## 4. Definition of Ready (Story Level)

A story proceeds only if ALL are met (in current mode — Core §1 relaxes in DISCOVERY/PROTOTYPE). Else mark `HOLD`.

- [ ] Persona identified (specific role)
- [ ] "So that" clause present
- [ ] Acceptance criteria (Gherkin with alt/error flows)
- [ ] Data contracts/interfaces specified or referenced
- [ ] External dependencies identified with availability
- [ ] Security/compliance flags noted
- [ ] Team can estimate it (else SPIKE)
- [ ] INVEST: Independent ✓ | Negotiable ✓ | Valuable ✓ | Estimable ✓ | Small ✓ | Testable ✓

---

## 5. Solution Design Quality Gate

Required sections: Business Context, Architecture Overview, Data Models, APIs/Interfaces, User Journeys.

If > 2 missing: `DESIGN INCOMPLETE: Missing [sections].`
If none present: `ERROR: No Solution Design detected.`

**Health Score (1–3 each, total /9):** Clarity, Feasibility, Completeness.
**Proceed** if ≥ 7/9 and no High risk without mitigation. **Hold** otherwise.

Additional analysis: STRIDE threat assessment, compliance audit, Domain Glossary extraction (Core §2 P3), scalability/token-limit assessment.

---

## 6. Conventions

- **Branch:** `feature/[STORY-ID]-short-description`
- **Commits:** `[type]([scope]): [summary]`
- **IDs:** EPICs A, B, C…; Stories A-1, A-2…; Tasks A-1-T1; Sub-tasks A-1-T1-ST1
- **Domain Language:** all code uses Glossary terms — no synonyms (Core §2 P3)

---

## 7. Backlog Templates

### 7.1 EPIC
- **Objective:** one sentence
- **Tracer Bullet?:** YES (EPIC 1) | NO
- **Mode:** DISCOVERY | PROTOTYPE | DELIVERY | HARDENING (Core §1)
- **Definition of Done:** ≤ 3 verifiable bullets (additions to Global DoD — Core §3)
- **Business Value:** linked to KPIs; quantified success metric
- **Risk Assessment:** High/Med with mitigations or `Accepted until [date] — [rationale] — Owner: [name]`
- **SLO Definitions:** `Service – Metric < Threshold over Window`
- **Domain Glossary:** terms for this EPIC
- **Assumptions Ledger:** High-Impact items flagged
- **ADRs:** referenced IDs (Core §2 P6)
- **Smoke Test Story:** at least one E2E smoke test
- **Runbook Link:** symptoms, checks, logs/metrics, rollback

### 7.2 USER STORY
- **ID & Title**
- **Persona Narrative:** As a [Persona] I want [Action] so that [Benefit]
- **Business Value:** High=3 / Med=2 / Low=1
- **Priority Score (1–5):** Business Value + inverse Risk + Dependency impact + SLO impact
- **Acceptance Criteria:** Gherkin (Given/When/Then) with alt/error flows
- **INVEST Check:** I✓/✗ | N✓/✗ | V✓/✗ | E✓/✗ | S✓/✗ | T✓/✗
- **Size:** S / M / L / XL (relative — no day mapping; Core §2 P9)
- **External Dependencies** with availability
- **Technical Debt Considerations:** potential TD Items with preliminary principal/interest
- **Test Data Requirements**
- **Idempotency Strategy** (write paths)
- **DoR Status:** READY | HOLD (with gaps)

### 7.3 TASK
- **ID:** A-1-T1
- **Hat:** FEATURE | REFACTORING | PREPARATORY (Core §2 P2)
- **Goal:** imperative verb phrase
- **Token/Size Budget:** ≤ 15k; split if > 1,500 LOC or > 3 complex modules
- **TDD Execution Order:** (1) failing tests (2) simplest passing code (3) refactor (Core §2 P1)
- **Required Interfaces:** Contract Snapshots or inline contracts (Core §4.1)
- **Abstraction Check:** patterns to reuse; extract if second occurrence (Core §5.5)
- **Deliverables:** test files → implementation → migrations → docs (TDD order)
- **Quality Gates:** additions to Global DoD (Core §3)
- **Feature Flag:** name + default (FEATURE tasks with user-facing changes)
- **Rollback Notes**
- **Hand-Off Artifacts** for downstream tasks
- **Pull Gate:** upstream assumptions to verify (Core §2 P8)
- **Unblocks:** task IDs enabled; `END OF STORY SEQUENCE` where applicable
- **Confidence:** High=3 / Med=2 / Low=1 (Low → refinement task)
- **TD Created:** TD Item reference if any shortcut (Core §2 P4)
- **Parallelisation:** SUB-TASKs only when truly independent. Never split coupled stateful logic.

### 7.4 SUB-TASK (optional)
- **ID:** A-1-T1-ST1 — relevant TASK fields

---

## 8. Special Story Types

**Spike:** `SPIKE: Research [topic]` — timeboxed S/M, findings + recommendation, follow-up story or rejection.

**Technical Debt Item:** `TD-[n]: [Component] — [Problem]` — artifact, type, cause, principal, recurring interest, compounding, servicing decision, origin task. (Core §2 P4 format.)

**Preparatory Refactoring:** `PREP: Restructure [Component] for [task ID]` — Hat: PREPARATORY, links to feature task, existing tests unchanged.

---

## 9. Validator

Reject or fix before returning if any fail:

**Structure:** DAG with root stories per EPIC. EPIC 1 is tracer bullet. Every task has Unblocks + Pull Gate. Token budgets respected.

**Quality:** Every story passes DoR or is HOLD. Every task declares Hat. TDD order (tests before implementation). No Two Hats violations. No pattern duplication without extraction. Glossary terms consistent. ADRs for cross-cutting decisions.

**Testing:** Critical paths tested at all layers. External integrations have contract tests + mock. Schema changes have migration + rollback + flag. E2E smoke test per EPIC.

**Risk & Debt:** Risks ≥ Medium have mitigation or accepted-with-date. PII → anonymised data + retention. Every shortcut → TD Item. Assumptions Ledger present.

**Operations:** SLO definitions per EPIC. Runbook per EPIC. Feature flags for user-facing changes. Idempotency keys for write paths.

**Economics (Core §5):** Anti-bureaucracy test: no task spec longer than its expected code output. No over-decomposition below token budget floor.

---

## 10. Iteration Workflow

1. Generate initial backlog (≤ 2 EPICs). EPIC 1 = tracer bullet.
2. Execute EPIC 1 task-by-task, pull-based.
3. After EPIC 1: produce Phase Summary + update Architecture Memory (Core §4).
4. Retrospective: rework rate, budget breaches, assumption failures, cycle time data, waste signals, mode check.
5. Refine remaining backlog. Expand scope. Repeat.

---

## 11. Solution Design Template

1. Business Context (problem, personas, KPIs, timeline)
2. Architecture Overview (components, relationships, stack)
3. Data Models (entities/ERD, attributes, sample payloads)
4. APIs & Interfaces (endpoints, schemas, errors, auth)
5. Non-Functional Requirements (performance, scalability, security, compliance)
6. Known Constraints (budget, legacy, skills)
7. User Journeys (primary flows, critical paths, exceptions)
8. Domain Glossary (key terms with precise definitions)

# AI-Native Software Delivery Core Specification v1.0

> **Purpose:** Shared foundation referenced by all execution documents (GPM, Backlog Builder, Partner Agent). Defines principles once. Eliminates redundancy. Provides adaptive rigor through execution modes.

---

## 1. Execution Modes

Not all work requires the same rigor. The framework operates in four modes with progressively stricter governance. **Declare the mode at the start of every EPIC or engagement.** Mode can change between EPICs but not within one.

### DISCOVERY Mode
*"We don't know what to build yet."*

**When:** Product discovery, market exploration, unclear domain, first contact with a problem space.

| Aspect | DISCOVERY |
|---|---|
| Domain Glossary | Draft — evolving, not enforced |
| Architecture | Deliberately deferred — no ADRs yet |
| DoR | Lightweight: persona + goal + one AC is enough |
| DoD | Code runs. Manual testing acceptable. No coverage threshold. |
| TDD | Optional — write tests for complex logic only |
| Two Hats | Not enforced — exploratory code mixes freely |
| Technical Debt | Not tracked per item — all DISCOVERY output is assumed disposable |
| Pull Gates | Not required |
| Fitness Functions | Not required |
| Output | Throwaway prototypes, validated learnings, refined hypotheses |

**Exit to PROTOTYPE:** When domain concepts stabilise and the team can name the core entities.

### PROTOTYPE Mode
*"We know roughly what to build. We're testing whether our approach works."*

**When:** Architecture exploration, tracer bullet construction, PoC for key technical risks.

| Aspect | PROTOTYPE |
|---|---|
| Domain Glossary | Required — first stable version. Enforced in code names. |
| Architecture | Tracer bullet required. Key ADRs for irreversible decisions only. |
| DoR | Standard: persona + AC + interfaces referenced |
| DoD | Tests for core logic. No coverage threshold. Feature flags optional. |
| TDD | Required for business logic. Optional for glue/wiring code. |
| Two Hats | Enforced for core domain code. Relaxed for infrastructure/wiring. |
| Technical Debt | Tracked for architecture-level shortcuts only. Code debt assumed. |
| Pull Gates | Required between components that depend on each other |
| Fitness Functions | Dependency rule check only |
| Output | Working tracer bullet, stable glossary, key ADRs, validated architecture |

**Exit to DELIVERY:** When tracer bullet works end-to-end and architecture is confirmed.

### DELIVERY Mode
*"We know what to build and how. Now we build it right."*

**When:** Feature development on a validated architecture. The main operating mode for most projects.

| Aspect | DELIVERY |
|---|---|
| Domain Glossary | Enforced — all code, all prompts, all documentation |
| Architecture | Full ADR set. Fitness functions in CI. |
| DoR | Full: INVEST check, AC in Gherkin, interfaces specified, dependencies confirmed |
| DoD | Full Global DoD. Coverage thresholds. Contract tests. Feature flags. |
| TDD | Mandatory — tests before implementation, every task |
| Two Hats | Mandatory — every task declares FEATURE/REFACTORING/PREPARATORY |
| Technical Debt | Full tracking: artifact, type, cause, principal, interest, servicing decision |
| Pull Gates | Mandatory on every task |
| Fitness Functions | Full set: dependency rule, contracts, performance, security |
| Output | Production-quality increments meeting full DoD |

**Exit to HARDENING:** When feature-complete and preparing for release or compliance review.

### HARDENING Mode
*"We're preparing for production, compliance, or long-term operation."*

**When:** Pre-release hardening, security audit, performance tuning, compliance certification.

| Aspect | HARDENING |
|---|---|
| Domain Glossary | Frozen — no new terms without formal review |
| Architecture | ADRs reviewed and confirmed. No new architectural changes. |
| DoR | Full + security review + performance baseline required |
| DoD | Full + performance meets SLO + security scan clean + runbook complete |
| TDD | Mandatory. Performance tests with numeric thresholds. |
| Two Hats | REFACTORING and PREPARATORY only — no new features |
| Technical Debt | All HIGH items must have servicing decision. Debt register reviewed. |
| Pull Gates | Mandatory. Rollback plan verified before each task. |
| Fitness Functions | All active + continuous monitoring in staging/production |
| Output | Hardened, documented, operable system with runbooks and SLO verification |

### Mode Transition Rules

```
DISCOVERY → PROTOTYPE:  Domain concepts named; core entities identified
PROTOTYPE → DELIVERY:   Tracer bullet works; architecture confirmed; glossary stable
DELIVERY → HARDENING:   Feature-complete; preparing for release/compliance
Any → DISCOVERY:        Major pivot; fundamental assumptions invalidated
HARDENING → DELIVERY:   Post-release; returning to feature development
```

Modes are not maturity levels. A mature product returning to explore a new market enters DISCOVERY for that exploration while the main product stays in DELIVERY.

---

## 2. Shared Principles (referenced by all documents)

These apply in all modes unless the mode table above explicitly relaxes them.

### P1: Tests Drive Design
Tests are written before implementation. The execution order is Red → Green → Refactor. Tests are not a quality gate applied after the fact — they are the specification that drives the design. Relaxed only in DISCOVERY mode.

### P2: One Hat Per Unit of Work
Every task/prompt declares FEATURE, REFACTORING, or PREPARATORY. Never mix structural change with behaviour change in the same unit. Relaxed only in DISCOVERY mode.

### P3: Domain Language Is Code Language
The Domain Glossary governs all generated code. One term per concept, one concept per term. The glossary starts as a draft in DISCOVERY and becomes enforced from PROTOTYPE onward.

### P4: Debt Is Visible or It Compounds
Any known shortcut produces a Technical Debt Item. The item tracks: artifact, type, cause, principal, recurring interest, compounding status, servicing decision. Tracked from PROTOTYPE onward; full tracking in DELIVERY and HARDENING.

### P5: Dependencies Point Inward
The Dependency Rule is structural, not stylistic. Inner layers define interfaces; outer layers implement them. Domain never imports infrastructure. Enforced from PROTOTYPE onward.

### P6: Architectural Decisions Are Recorded
Cross-cutting decisions (auth, data store, messaging, error strategy, observability, API conventions) require ADRs with context, alternatives considered, consequences accepted, and review date. Irreversible decisions recorded from PROTOTYPE; full ADR set from DELIVERY.

### P7: Properties Are Verified Automatically
An architectural property stated but not automatically checked will degrade silently. Every important property needs a fitness function. Dependency rule enforced from PROTOTYPE; full fitness function set from DELIVERY.

### P8: Pull Before Proceeding
Before starting any task, verify upstream assumptions still hold. If interfaces changed, update before executing. Not applied in DISCOVERY; required from PROTOTYPE onward.

### P9: No Single-Point Estimates
Sizing is relative (S/M/L/XL) without calendar-day mapping. Forecasting uses observed cycle time percentiles, not predictions. Commitments state ranges with confidence levels. Applied in DELIVERY and HARDENING.

### P10: Waste Is Named and Eliminated
The seven software wastes (partially done work, extra processes, extra features, task switching, waiting, handoffs, defects) are tracked in retrospectives. The biggest waste source is almost always waiting — address it first. Applied from PROTOTYPE onward.

---

## 3. Global Definition of Done

Applied in DELIVERY and HARDENING modes. PROTOTYPE uses a subset (marked with *).

- [ ] Tests written first (Red-Green-Refactor), all passing *
- [ ] No hardcoded secrets, tokens, or credentials *
- [ ] Lint and format clean *
- [ ] New logic has ≥ 80% unit test coverage
- [ ] Contract tests pass for any external integration
- [ ] Feature flag declared for user-facing changes
- [ ] No new technical debt without a corresponding TD Item *
- [ ] Hand-off artifacts present for downstream tasks
- [ ] No pattern duplicated from previous task without extraction
- [ ] Domain Glossary terms used consistently *
- [ ] Integration Note present (plain-language business context)

---

## 4. Context Compression Strategy

LLM context windows are finite. Long projects will exceed them. These mechanisms prevent context loss without requiring the full project history in every prompt.

### 4.1 Contract Snapshots
After each component is accepted, produce a **Contract Snapshot**: a minimal document containing only the component's public interface (types, function signatures, error shapes). No implementation details. This snapshot is what downstream tasks reference — not the full source code.

```
CONTRACT SNAPSHOT: [Component Name]
Version: [n]
Date: [date]

Public Interface:
  [function signatures with types]

Error Shapes:
  [error types and when they occur]

Dependencies:
  [what this component requires]

Domain Terms Used:
  [glossary terms this component operates on]
```

Contract Snapshots are the **minimum context** needed for downstream tasks. If a task only needs to call a component, it only needs the snapshot — not the implementation.

### 4.2 Architecture Memory
A living document (updated per EPIC) that captures the current state of the system at a level an AI agent can consume in one prompt:

```
ARCHITECTURE MEMORY: [System Name]
Updated: [date]

Components:
  [name]: [one-line purpose] — [status: stable/in-progress/planned]

Key ADRs (current):
  ADR-[n]: [decision — one line]

Domain Glossary (current):
  [term]: [definition — one line each]

Integration Map:
  [A] → [B]: [pattern — sync/async/event]

Active Technical Debt:
  TD-[n]: [component — problem — interest/sprint]

Current Mode: [DISCOVERY/PROTOTYPE/DELIVERY/HARDENING]
```

This document replaces "re-read the entire project history." It is the compressed state of the system.

### 4.3 Summarisation Checkpoints
After each EPIC completes, produce a **Phase Summary** that captures:
- What was built (component list with one-line descriptions)
- What was learned (assumptions validated/invalidated)
- What changed from the plan (interface modifications, scope adjustments)
- Current debt register (active TD Items)
- Updated Architecture Memory

The Phase Summary replaces the full EPIC history for future context. Previous EPIC details can be dropped from context as long as the Phase Summary and Contract Snapshots are retained.

### 4.4 Context Budget Per Task
Each task should be executable with:
- The Architecture Memory (~500 tokens)
- Relevant Contract Snapshots (~200 tokens each, typically 1-3)
- The task specification itself (~1,000-2,000 tokens)
- The Global DoD (~200 tokens)

**Target: any task executable within 3,000 tokens of project context.** If a task requires more context than this, it's either too broadly scoped or the Architecture Memory needs updating.

---

## 5. Economic Heuristics

The framework tells you WHAT to do. These heuristics tell you WHEN it's worth doing.

### 5.1 When Rigor Is Worth It

| Signal | Increase rigor | Decrease rigor |
|---|---|---|
| System lifetime | > 2 years | < 6 months |
| Team size | > 3 people | Solo developer |
| Domain complexity | Regulated, safety-critical, financial | Internal tool, throwaway |
| Change frequency | Core business logic | Configuration, styling |
| Blast radius | Affects many users/systems | Affects only the developer |
| Reversibility | Hard to undo (data migration, public API) | Easy to undo (feature flag, internal refactor) |

**The rule of thumb:** Apply maximum rigor to things that are expensive to get wrong and hard to fix later. Apply minimum rigor to things that are cheap to redo.

### 5.2 When Debt Is Acceptable

Debt is a tool, not a failure. Accept debt when:
- The system area is not evolving (no accruing interest)
- The principal exceeds the total future interest (cheaper to live with it)
- You're in DISCOVERY or PROTOTYPE mode (the code is assumed disposable)
- The market window matters more than the code quality (deliberate business decision — but record it)

Debt is NOT acceptable when:
- It's invisible (not tracked → not managed → compounds silently)
- It's in the Core Domain (highest-traffic, highest-value code)
- It affects security or data integrity
- Future work will build on top of it (compounding = true)

### 5.3 When to Stop Decomposing

A task is small enough when:
- An AI agent can execute it in one prompt (≤ 15k tokens output)
- It has a clear single purpose (one Hat, one goal)
- Its test expectations fit in 10-15 test cases
- Its interfaces are fully specified

Stop decomposing when further splitting would:
- Create tasks that can't be understood in isolation
- Introduce artificial boundaries within cohesive logic
- Generate more coordination overhead than they save
- Produce tasks smaller than the DoR/DoD overhead

**The anti-bureaucracy test:** If the task specification is longer than the code it would produce, you've over-decomposed.

### 5.4 When to Merge Tasks

Merge when:
- Two tasks always change together (Common Closure)
- One task's output is consumed only by the immediately next task with no other dependents
- The combined task is still within token/LOC budget
- Splitting them creates a handoff that loses context

### 5.5 When to Skip the Abstraction Check

Skip DRY extraction when:
- The pattern has appeared only once (premature abstraction is worse than duplication)
- The two instances are in different Bounded Contexts (duplication across contexts is often correct)
- Extracting would create a dependency between components that should be independent
- You're in DISCOVERY mode (duplication is cheaper than wrong abstractions)

**Rule of Three (from Fowler):** First time, do it. Second time, wince but do it. Third time, extract.

---

## 6. Agent Capability Profiles

Different models have different strengths. Match the model to the task.

### Reasoning-Heavy (Opus-class)
**Use for:** Solution design, architecture decisions, domain modelling, trade-off analysis, backlog generation, complex debugging, code review with architectural implications.

**Characteristics:** Deep reasoning, long context handling, nuanced judgment, slower, higher cost.

**Agents that benefit:** `quality-attribute-analyzer`, `architecture-assessment-facilitator`, `core-domain-identifier`, `technical-debt-strategist`, `product-owner-coach`, `backlog-builder`, `gpm-partner` (for prompt generation, not code execution).

### Execution-Focused (Sonnet-class)
**Use for:** Code generation from well-specified tasks, test writing, refactoring with clear mechanics, migration scripts, documentation generation.

**Characteristics:** Fast, capable, good at following structured specifications, cost-effective at volume.

**Agents that benefit:** `tdd-practitioner`, `refactoring-catalog-advisor`, `code-smell-detector`, `fowler-smell-detector`, `naming-reviewer`, `function-analyzer`, `gpm-partner` (for code execution from prompts).

### Fast/Lightweight (Haiku-class)
**Use for:** Lint checks, format verification, simple classification tasks, DoR/DoD checklist verification, glossary term lookups, contract snapshot generation.

**Characteristics:** Very fast, very cheap, good for high-volume low-complexity checks.

**Agents that benefit:** `ubiquitous-language-guard` (term checking only), fitness function verification, pre-commit hook checks.

### The Matching Rule
```
If the task requires JUDGMENT → Opus
If the task requires GENERATION from a clear spec → Sonnet
If the task requires VERIFICATION against a checklist → Haiku
```

When uncertain: start with Sonnet. Escalate to Opus if the output quality is insufficient. Never use Opus for tasks that Sonnet handles well — the cost difference matters at scale.

---

## 7. Document Relationships

```
                    ┌─────────────────────────┐
                    │  CORE SPECIFICATION      │ ← this document
                    │  (shared principles,     │
                    │   modes, DoD, economics) │
                    └──────────┬──────────────┘
                               │
              ┌────────────────┼────────────────┐
              │                │                │
     ┌────────▼──────┐  ┌─────▼──────┐  ┌──────▼───────┐
     │   GPM v2      │  │ Backlog    │  │ Agent Suites │
     │ (methodology) │  │ Builder v5 │  │ (59 agents)  │
     └───────┬───────┘  │ (framework)│  └──────────────┘
             │          └─────┬──────┘
     ┌───────▼───────┐  ┌────▼────────┐
     │ GPM Partner   │  │ Backlog     │
     │ Agent         │  │ Builder     │
     │ (execution)   │  │ Agent       │
     └───────────────┘  └─────────────┘
```

**How to read this:**
- The Core Specification defines principles, modes, DoD, economics, and compression strategy ONCE
- GPM v2 and Backlog Builder v5 reference the Core Specification instead of re-defining shared concepts
- The agents (GPM Partner, Backlog Builder) implement the methodology documents
- The 59 suite agents are called as specialists during execution

**Maintenance rule:** When a shared principle changes, update the Core Specification. The methodology documents and agents inherit the change. Never update the same principle in multiple documents.

# The Guided Partnership Model (GPM) v2.1

> **Depends on:** `core-specification-v1.md` for shared principles, execution modes, DoD, compression, and economics.
> **Executed by:** `gpm-partner-agent.md`

---

## 1. What GPM Adds Beyond the Core

The Core Specification defines principles and quality standards. GPM defines **how humans and AI collaborate** — roles, prompt types, phase workflow, and stakeholder integration.

---

## 2. Roles

- **Architect:** System design, prompt strategy, Domain Glossary ownership, ADR authorship
- **Stakeholder:** Business context, value hypotheses, validation criteria, smoke-test scripts
- **Reviewer:** DoD verification, quality assurance, acceptance

---

## 3. Prompt Types

| Type | Purpose | Hat (Core §2 P2) |
|---|---|---|
| **ZAP** | Build a single component — zero assumptions about implementation | FEATURE or REFACTORING |
| **CIP** | Connect validated components into integrated flows | FEATURE |
| **PREP** | Restructure existing code to make an upcoming ZAP easier | PREPARATORY |
| **SPIKE** | Time-boxed exploration — throwaway code, validated learning | N/A |

---

## 4. Phase Workflow

**Declare the execution mode (Core §1) at the start of each EPIC.**

### Phase 0: Domain Foundation

**Architect produces:**
1. Domain Glossary — every entity, role, status, action (Core §2 P3)
2. Architecture Overview — components, responsibilities, interface contracts
3. Technology Stack declaration
4. Cross-cutting ADRs (Core §2 P6) — auth, error format, logging, data access, API conventions

**Stakeholder produces:**
5. Value Hypothesis per feature: "We believe [X] will achieve [Y] for [persona]. Measured by [metric]."
6. Smoke-Test Outlines — 3–5 step plain-language validation for critical journeys

**Exit:** Glossary reviewed and approved by both Architect and Stakeholder.

### Phase 1: Tracer Bullet

Thin end-to-end working slice through all layers. Not scaffolding — deployable production code.

**Includes:** one working endpoint (request → logic → persistence → response), core entity schema + migration + seed data, health check, CI pipeline (build + test + lint), structured logging.

**Prompt sequence:** 5 ZAPs in strict order (project structure → entity model → repository + tests → endpoint + integration test → health check + CI).

**Exit:** Deploys. Serves one request end-to-end. Tests pass. Stakeholder's simplest smoke test passes.

### Phase 2: Core Logic (TDD-Driven)

Build remaining business logic component by component.

**ZAP Template:**
```markdown
# ZAP: [Component Name]
## Hat: [FEATURE | REFACTORING]
## Domain Context: [Glossary terms this component uses]
## Requirements: [numbered list]
## Input/Output Contract: [typed interfaces]
## Business Rules: [numbered, domain-specific]
## Test Expectations:
  Happy Path: [specific scenarios]
  Error Conditions: [each failure mode]
  Edge Cases: [boundaries]
  Performance: [numeric thresholds if applicable]
## Constraints: [libraries, patterns, security]
## Dependencies: [component contracts — use Contract Snapshots per Core §4.1]
## Abstraction Check: [patterns from previous components to reuse]
## Security Considerations: [PII, auth, validation]
## Error Handling: [reference cross-cutting ADR]
```

**Between ZAPs:** Run Pull Gate (Core §2 P8). Produce Contract Snapshot (Core §4.1) for accepted output.

### Phase 3: Integration & Wiring

Connect validated components using CIPs.

**CIP Template:**
```markdown
# CIP: [Integration Name]
## Hat: FEATURE
## Integration Context: [what's connected and why]
## Component Contracts: [verified Contract Snapshots]
## Dependency Wiring: [instantiation, injection, config]
## API Surface: [endpoints, middleware, request/response flow]
## Integration Test Expectations: [end-to-end scenarios]
## Configuration: [env vars, feature flags]
## Observability: [logging, metrics, tracing, health checks]
## Feature Flags: [user-facing changes with defaults]
## Rollback Plan: [how to disable without breaking]
```

**If restructuring needed:** Issue a PREP prompt before the CIP:
```markdown
# PREP: Restructure [Component] for [CIP name]
## Hat: PREPARATORY
## What Changes: [structural change]
## Why: [which CIP this prepares for]
## Constraint: Existing tests pass unchanged. No new behaviour.
```

### Phase 4: Hardening & Validation

Performance tests against SLO thresholds. Security review (STRIDE verified against implementation). Stakeholder smoke-test execution. E2E automation. Runbook creation. API documentation.

**Value Validation:** Does the implementation match the Vision? Can we measure the Value hypothesis? When does real user feedback arrive?

**Exit:** Update Architecture Memory (Core §4.2). Produce Phase Summary (Core §4.3).

---

## 5. Collaborative Layer

### Stakeholder Participation Per Phase

| Phase | Stakeholder Role | Artifact |
|-------|-----------------|----------|
| 0 | Define value hypotheses, write smoke-test outlines, approve Glossary | Value Hypotheses, Smoke Scripts |
| 1 | Execute simplest smoke test against tracer bullet | Pass/fail + feedback |
| 2 | Review Integration Notes per component | Business context verification |
| 3 | Verify integrated flows match business expectations | Acceptance walk-through |
| 4 | Execute full smoke-test scripts, confirm value measurement | Final validation |

### Integration Note Template
```
This [component] feeds into [business process].
It must [critical business rule].
When it fails, [business impact].
Domain terms: [list from Glossary].
```

### Prompt Peer-Review
1. Architect writes ZAP
2. Stakeholder reads and flags anything unclear or business-incorrect
3. Both verify Domain Glossary terms
4. Both confirm Test Expectations cover real scenarios
5. Prompt marked READY (DoR met)

### Escalation Triggers

| Condition | Action |
|-----------|--------|
| Rework rate > 30% | ZAPs are underspecified — review prompt quality |
| Cycle Time trending up | Investigate: complexity? debt? fatigue? mode mismatch? |
| Stakeholder smoke test fails on accepted component | Integration Note gap |
| TD interest exceeding 20% of effort | Dedicated REFACTORING phase or escalate mode to HARDENING |
| Domain term inconsistency in generated code | Glossary enforcement gap |

---

## 6. Governance Gates

**Phase 0 exit:** Glossary approved. ADRs documented. Value hypotheses defined. Smoke outlines written.

**Phase 1 exit:** Tracer bullet deploys and serves end-to-end. Tests pass. Smoke test passes. CI runs.

**Phase 2 exit (per component):** DoR met. TDD order followed. DoD met (per current mode — Core §1). Integration Note reviewed. Abstraction Check passed. Contract Snapshot produced.

**Phase 3 exit:** Integration tests pass end-to-end. Feature flags declared. Rollback plan documented.

**Phase 4 exit:** SLOs verified. Security review complete. Smoke tests pass. Runbook created. Architecture Memory updated. Phase Summary produced.

---

## 7. Flow Metrics

Track per prompt execution: start time, end time, Component Cycle Time, rework (YES/NO + cause).

After 5+ components: calculate 50th and 85th percentile CT.

**Commitment format:** "Based on our last N components, a new component has an 85% probability of acceptance within [85th percentile CT]."

---

## 8. Retrospective Template

After each phase:

- **Flow:** avg CT, trend, outliers
- **Quality:** rework rate, top rework causes, DoD violations caught
- **Waste:** partially done work, extra features generated, waiting time, context loss, duplication
- **Debt:** new TD Items, total interest, debt ratio (flag if > 20%)
- **Domain:** glossary terms added, drift incidents
- **Mode check:** Is current mode still appropriate? Should we escalate/relax?
- **Actions:** top 3 improvements for next phase

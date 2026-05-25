# Impact-Aware Contract Drift - Enterprise Build-Up Plan

> Status: draft v0.1  
> Date: 2026-05-21  
> Audience: architecture, product, platform engineering, delivery leads  
> Purpose: turn Radar's differentiator from MVP feature set into a publishable enterprise-grade product capability.

---

## 1. Executive Summary

Radar should not compete as another generic OpenAPI diff tool. That space is already strong, with mature tools such as oasdiff, Optic, Pact/PactFlow, Schemathesis, Postman, Stoplight, and Backstage-adjacent API governance stacks.

Radar's defensible position is:

> Before an API change is merged, Radar tells the producer which real consumers are at risk, why Radar believes that, what evidence supports the claim, and what action should happen next.

The current codebase already contains the core nouns:

- spec diffs
- consumers
- subscriptions
- runtime usage events
- static call sites
- blast radius
- generated tests
- release notes
- CI policy
- PR comments

The gap is not conceptual. The gap is production confidence, evidence quality, enterprise trust, and workflow packaging.

This document defines the build path from "promising MVP" to "publishable enterprise differentiator."

---

## 2. Product Thesis

### 2.1 The market problem

API diff tools answer:

- What changed?
- Is the change structurally breaking?

Enterprise teams need:

- Who is affected?
- Is that consumer actively using the changed field?
- Which repo, owner, team, and call site are involved?
- Can the producer merge safely?
- If not, what exact migration work is required?
- Can the decision be audited later?

### 2.2 Radar's differentiated promise

Radar becomes the system of record for API change impact:

1. Detect candidate breaking changes from specs.
2. Resolve impact using runtime and static evidence.
3. Produce a named, evidence-backed blast radius.
4. Gate CI based on enterprise policy.
5. Generate migration artifacts for affected teams.
6. Preserve the decision trail for audit and governance.

### 2.3 Non-goals

Radar should not try to become:

- a full API design platform like Stoplight/Postman
- a full developer portal like Backstage
- a full consumer-driven contract platform like PactFlow
- a full observability platform like Datadog/New Relic
- a general LLM test-generation product

Radar should integrate with those systems where useful. Its product center is impact-aware change control.

---

## 3. Publishable Status Definition

"Publishable" means Radar can be credibly shown, documented, and piloted by an enterprise platform team without apologizing for core product gaps.

### 3.1 Publishable demo criteria

A public or customer-facing demo must show:

1. A producer PR removes or changes an API field.
2. Radar detects the breaking change.
3. Radar names affected consumers.
4. Radar shows runtime usage evidence and static call-site evidence.
5. Radar links to repo/team/contact metadata.
6. Radar posts a PR comment with an actionable decision.
7. Radar blocks or warns according to policy.
8. Radar generates migration notes and targeted tests.
9. The dashboard shows the diff, affected consumers, evidence, and audit state.

### 3.2 Enterprise pilot criteria

An enterprise pilot must additionally show:

1. Multi-tenant org isolation across all resources.
2. OIDC/JWT authentication with no static fallback secrets.
3. Postgres-backed API tests and migration checks.
4. GitHub Action packaging with fail-open/fail-closed controls.
5. Runtime event ingestion from at least one low-friction source.
6. Service ownership ingestion from repo metadata or Backstage.
7. Configurable policy with audit trail.
8. Data retention controls for usage and call-site evidence.
9. SBOM, vulnerability audit, and release versioning.

### 3.3 Public positioning criteria

The README, website, and docs can claim "impact-aware contract drift" only when:

- blast radius uses both runtime and static evidence
- confidence levels are explainable
- CI gating is deterministic and configurable
- generated artifacts are tied to the actual diff and affected consumers
- enterprise setup is documented end to end

---

## 4. Current State Assessment

### 4.1 Strong existing foundations

| Area | Current strength |
|---|---|
| Diff engine | Multi-format support exists; OpenAPI coverage has improved. |
| Consumer model | Producer, consumer, subscription, repo URL, owner team, and contact fields exist. |
| Evidence model | Runtime `usage_event` and static `call_site` paths exist. |
| Blast radius | API resolves changed operations and fields into affected consumers with evidence. |
| CI | CLI can post diffs, fetch blast radius, and exit by policy. |
| UI | Dashboard and diff views expose the right concepts. |
| Generated artifacts | Test generation and release notes exist. |
| Operations | CI, audit, SBOM, health, metrics, and runbook have started. |

### 4.2 Main weaknesses

| Weakness | Why it matters |
|---|---|
| Evidence collection is opt-in and thin | Without easy runtime capture, many pilots will show empty blast radius. |
| Static scanner lacks operation context | Field-only matches can create false positives. |
| Generated tests are not impact-targeted | Tests are generated from Jira/spec, not from affected fields and consumers. |
| CI action is not productized | Enterprise users expect a drop-in GitHub Action, not a bespoke CLI command. |
| Policy semantics need hardening | Fail-open behavior must be explicit, not accidental. |
| Enterprise identity and tenancy are incomplete until fully audited | Multi-tenant claims require complete isolation. |
| Backstage/catalog ownership ingestion is absent | Enterprise teams do not want to maintain ownership twice. |
| Decision audit workflow is underdeveloped | Approvals, overrides, acknowledgements, and expiry need first-class records. |

---

## 5. Target Capability Model

### 5.1 Capability: Evidence-backed blast radius

#### Product outcome

For every risky API change, Radar can say:

- affected consumer
- owning team
- repo
- contact
- exact operation
- exact field path
- runtime last seen
- static call-site file and line
- confidence level
- policy verdict

#### Required design

Evidence should be normalized into an append-only model:

```text
impact_evidence
  id
  org_id
  diff_id
  change_id
  producer_service_id
  consumer_id
  source_type             runtime_usage | static_call_site | contract_test | manual_ack
  operation
  field_path
  confidence             high | medium | low
  evidence_uri           repo URL, trace URL, CI URL, test result URL
  file_path
  line_number
  observed_at
  expires_at
  metadata_json
```

The blast-radius endpoint should read from this model rather than recomputing everything ad hoc on every request. The recomputation step can still exist, but should produce durable evidence records.

#### Success criteria

- Every blast-radius entry has at least one evidence item.
- Evidence is explainable in the UI and PR comment.
- Runtime evidence outranks static evidence.
- Stale evidence automatically expires by retention policy.
- Evidence generation is testable with fixtures.

---

### 5.2 Capability: Runtime usage collection

#### Product outcome

Teams can adopt Radar without hand-writing usage event calls in every service.

#### Ingestion paths

| Path | Priority | Notes |
|---|---:|---|
| OpenTelemetry collector exporter or processor | P0 | Best enterprise fit. Reuses existing telemetry pipelines. |
| HTTP middleware SDKs | P1 | Start with Node/Express, FastAPI, Spring Boot. |
| API gateway export | P1 | Kong, Envoy, NGINX, APISIX, AWS API Gateway logs. |
| Manual `/usage/events` API | Existing | Keep for tests and advanced users. |

#### Required event contract

```json
{
  "producer_service_id": "payments-api",
  "consumer_id": "billing-svc",
  "operation": "GET /users/{id}",
  "field_path": "response.user.phone",
  "source": "otel",
  "trace_id": "optional",
  "span_id": "optional",
  "observed_at": "2026-05-21T10:00:00Z"
}
```

#### Important design choice

Do not require payload capture for production usage. Payload capture creates privacy, PII, and security barriers. Prefer field-path summaries and operation labels. If payload sampling is added later, make it opt-in, redacted, and short-lived.

---

### 5.3 Capability: Operation-aware static scanning

#### Product outcome

Radar can identify consumer call sites with enough precision to reduce false positives.

#### Scanner maturity stages

| Stage | Capability | Publishable? |
|---|---|---:|
| S0 | Field property extraction only | No |
| S1 | HTTP client call extraction plus field extraction | Partial |
| S2 | Operation plus field correlation | Yes for pilot |
| S3 | Generated-client aware scanner | Strong |
| S4 | Framework-specific semantic scanner | Enterprise-grade |

#### Initial language targets

1. TypeScript/JavaScript
2. Python
3. Go
4. Java/Kotlin
5. C#/.NET

#### Scanner evidence examples

```text
consumer: billing-svc
operation: GET /users/{id}
field_path: response.user.phone
file: src/clients/users.ts
line: 84
confidence: medium
reason: field read from response object returned by known generated client method
```

#### Implementation notes

- Prefer generated-client detection over arbitrary HTTP heuristics.
- Let users provide client mapping config.
- Emit confidence reasons, not just scores.
- Scan changed consumer repos incrementally where possible.
- Do not block CI on low-confidence static-only matches unless policy opts in.

---

### 5.4 Capability: CI/PR productization

#### Product outcome

A platform team can install Radar into a producer repo in under 15 minutes.

#### Required GitHub Action interface

```yaml
- uses: radar-monitor/radar-action@v1
  with:
    base-spec: openapi.previous.yaml
    head-spec: openapi.yaml
    service-id: payments-api
    radar-url: https://radar.internal.example.com
    policy: active-consumers
    fail-mode: closed
    post-comment: true
  env:
    RADAR_TOKEN: ${{ secrets.RADAR_TOKEN }}
```

#### Required action outputs

```text
diff-id
breaking-count
affected-consumer-count
policy-verdict
dashboard-url
release-notes-url
```

#### PR comment sections

1. Verdict
2. Breaking changes
3. Affected consumers
4. Evidence table
5. Required acknowledgements
6. Generated migration notes
7. Override policy
8. Links to dashboard and artifacts

#### Fail-mode policy

| Mode | Behavior |
|---|---|
| `closed` | Any Radar API/evidence lookup failure blocks the build. |
| `open` | Structural diff still runs, but missing Radar API does not block. |
| `warn` | Never fails build; posts report only. |

Default enterprise mode should be `closed`.

---

### 5.5 Capability: Impact-targeted tests

#### Product outcome

Generated tests should prove the risky changed contract area, not merely create broad API examples.

#### Required flow

1. Diff identifies changed operation/field.
2. Blast radius identifies affected consumers.
3. Radar chooses test templates relevant to change kind.
4. Radar generates:
   - producer contract regression tests
   - consumer smoke tests where possible
   - Postman collection
   - api-testing YAML
   - optional Schemathesis seed configuration
5. CI links generated tests to the PR comment.

#### Test template examples

| Change kind | Generated test |
|---|---|
| response field removed | Assert old field absence/presence behavior depending on target compatibility policy. |
| request field became required | Negative test omits field and verifies documented error response. |
| enum value removed | Test each consumer-observed enum value. |
| status code removed | Assert supported status code behavior and fallback response. |
| auth scheme changed | Validate old auth fails only if deprecation/upgrade path is documented. |

#### Guardrail

LLMs may write test descriptions and boilerplate, but deterministic templates should own the mapping from change kind to test intent.

---

### 5.6 Capability: Release notes and migration guides

#### Product outcome

Radar generates consumer-facing artifacts that are accurate, scoped, and useful.

#### Required release-note sections

1. Summary
2. Compatibility verdict
3. Breaking changes
4. Affected consumers
5. Per-consumer migration checklist
6. Generated tests
7. Timeline and deprecation date
8. Owner and acknowledgement status

#### Migration guide data sources

- diff changes
- blast-radius evidence
- consumer repo URL
- owner team/contact
- static call-site snippets
- current policy
- known replacement fields if provided

#### Approval workflow

Release notes should move through states:

```text
draft -> reviewed -> published -> superseded
```

This makes the artifact enterprise-auditable instead of a one-off AI text blob.

---

### 5.7 Capability: Enterprise service ecosystem

#### Product outcome

Radar works across hundreds of internal services without duplicate ownership maintenance.

#### Ownership sources

| Source | Priority | Notes |
|---|---:|---|
| `catalog-info.yaml` / Backstage | P0 | Most direct fit for service ownership. |
| GitHub repo topics/custom properties | P1 | Good for lighter orgs. |
| CODEOWNERS | P1 | Useful fallback for team inference. |
| Manual registry | Existing | Keep as override. |
| CSV/API import | P2 | Helpful for enterprise migration. |

#### Required catalog fields

```yaml
apiVersion: backstage.io/v1alpha1
kind: Component
metadata:
  name: billing-svc
  annotations:
    radar.monitor/consumer-id: billing-svc
spec:
  type: service
  owner: team-billing
  system: commerce
```

#### Enterprise graph

Radar should represent:

```text
organization
  -> systems
    -> producer services
      -> API specs
      -> diffs
      -> consumers
        -> subscriptions
        -> usage evidence
        -> call-site evidence
        -> acknowledgements
```

---

## 6. Architecture Evolution

### 6.1 Current architecture

```text
radar-cli -> radar-api -> SQLite/Postgres
radar-scanner -> radar-api
radar-ui -> radar-api
```

### 6.2 Target enterprise architecture

```text
Producer CI
  -> radar-action
    -> radar-cli core
    -> radar-api
      -> diff engine
      -> policy engine
      -> evidence service
      -> artifact service
      -> audit service

Runtime telemetry
  -> OpenTelemetry Collector / gateway / SDK
    -> usage ingest
    -> evidence service

Consumer repos
  -> scanner worker
    -> call-site ingest
    -> evidence service

Service catalog
  -> Backstage/GitHub/CODEOWNERS importer
    -> service registry

Dashboard
  -> diff view
  -> blast radius view
  -> policy decisions
  -> generated tests
  -> release notes
  -> audit trail
```

### 6.3 Service boundaries

Keep one deployable `radar-api` binary for now, but make internal modules explicit:

| Module | Responsibility |
|---|---|
| `diffs` | Spec versions, diffs, changes. |
| `evidence` | Runtime usage, call sites, evidence normalization. |
| `impact` | Blast-radius computation and confidence scoring. |
| `policy` | CI verdicts, overrides, acknowledgements. |
| `artifacts` | Tests, release notes, migration guides. |
| `catalog` | Services, consumers, ownership import. |
| `authz` | Org isolation, roles, token/session checks. |
| `audit` | Append-only decision log. |

This avoids premature microservices while creating clear ownership boundaries.

---

## 7. Data Model Additions

### 7.1 Evidence table

Add `impact_evidence` as described in section 5.1.

### 7.2 Policy decision table

```text
policy_decision
  id
  org_id
  diff_id
  policy_name
  verdict                pass | warn | block | overridden
  fail_mode              open | closed | warn
  reason
  created_at
  created_by
  metadata_json
```

### 7.3 Acknowledgement table

```text
acknowledgement
  id
  org_id
  diff_id
  consumer_id
  change_id
  acknowledged_by
  acknowledgement_type   owner_ack | platform_override | expiry_override
  comment
  expires_at
  created_at
```

### 7.4 Artifact table

```text
artifact
  id
  org_id
  diff_id
  consumer_id nullable
  artifact_type          release_notes | migration_guide | postman_collection | apitesting_yaml | schemathesis_config
  status                 draft | reviewed | published | superseded
  content
  content_hash
  created_at
  updated_at
```

### 7.5 Catalog import table

```text
catalog_source
  id
  org_id
  source_type            backstage | github | codeowners | manual | csv
  source_url
  last_synced_at
  sync_status
  error_message
```

---

## 8. Confidence Scoring

### 8.1 Score inputs

| Signal | Weight | Notes |
|---|---:|---|
| Runtime usage within 7 days | High | Strongest signal. |
| Runtime usage within lookback window | Medium | Still relevant. |
| Static call site with operation context | Medium | Useful even without recent runtime traffic. |
| Static call site field-only | Low | Should not block by default. |
| Consumer-declared contract test | High | Strong if tied to same operation/field. |
| Manual subscription only | Informational | Shows relationship, not actual usage. |

### 8.2 Output format

```json
{
  "confidence": "high",
  "reason": "runtime usage of GET /users/{id} response.user.phone observed 2 days ago",
  "evidence": [
    {
      "kind": "runtime_usage",
      "operation": "GET /users/{id}",
      "field_path": "response.user.phone",
      "observed_at": "2026-05-19T09:12:00Z"
    }
  ]
}
```

### 8.3 Policy mapping

| Policy | Blocks on |
|---|---|
| `any_break` | Any structural breaking change. |
| `active_consumers` | High or medium confidence evidence. |
| `runtime_only` | Runtime evidence only. |
| `manual` | Never blocks automatically; requires review. |

---

## 9. Enterprise Security and Governance

### 9.1 Authentication

Required:

- OIDC for user sessions.
- JWT/service tokens for CI.
- No static fallback signing secret.
- Token rotation documentation.
- Scoped tokens per org/service.

### 9.2 Authorization

Every endpoint should enforce:

```text
org_id -> service_id -> resource_id
```

Audit all endpoints:

- diffs
- spec versions
- usage events
- call sites
- test suites
- release notes
- sandbox envs
- settings
- policies
- acknowledgements

### 9.3 Data privacy

Runtime usage must not require raw payload capture. Store:

- operation
- field path
- service identifiers
- timestamps
- trace IDs if available

Avoid storing:

- request bodies
- response bodies
- auth headers
- user identifiers
- PII values

### 9.4 Audit trail

Every production-relevant decision should be audit logged:

- policy verdict
- PR comment posted
- override label detected
- manual acknowledgement
- release notes published
- generated test artifact created

---

## 10. Delivery Roadmap

### Phase 1 - Differentiator Hardening

Goal: make the existing impact-aware capability reliable enough for internal dogfood.

#### Work items

1. Normalize blast-radius evidence into `impact_evidence`.
2. Add org-scoped audit tests for all core endpoints.
3. Add fail-mode support to CLI policy execution.
4. Make PR comment include evidence and policy verdict.
5. Add operation-aware scanner support for TypeScript generated clients.
6. Add fixtures for one complete demo scenario:
   - producer spec v1/v2
   - consumer repo fixture
   - usage event fixture
   - expected PR comment
   - expected dashboard output

#### Exit criteria

- One end-to-end test proves "field removed -> affected consumer -> evidence -> block".
- No blast-radius entry can be returned without evidence.
- CLI fail-open/fail-closed behavior is explicit and tested.

---

### Phase 2 - Enterprise Workflow Packaging

Goal: make Radar installable and usable by a platform team.

#### Work items

1. Create `radar-action` repository or workspace package.
2. Publish GitHub Action v0 with inputs/outputs.
3. Add PR annotations and markdown comment rendering.
4. Add dashboard links from comments.
5. Add policy decision records.
6. Add acknowledgement workflow:
   - acknowledge consumer impact
   - override with reason
   - expiry date
7. Add Backstage catalog importer.
8. Add docs:
   - 15-minute GitHub setup
   - Backstage ownership setup
   - OIDC setup
   - policy examples

#### Exit criteria

- A new repo can install Radar from docs without custom scripting.
- A PR comment clearly explains pass/warn/block.
- Overrides are auditable.

---

### Phase 3 - Runtime Evidence Collection

Goal: make adoption practical without manual event posting.

#### Work items

1. Build OpenTelemetry collector exporter or processor.
2. Add API gateway ingestion adapter.
3. Add SDK/middleware for Node/Express.
4. Add SDK/middleware for FastAPI.
5. Add SDK/middleware for Spring Boot.
6. Add ingestion sampling controls.
7. Add privacy/redaction documentation.
8. Add dashboard for evidence freshness and coverage.

#### Exit criteria

- At least one real service can produce usage evidence without custom application code beyond middleware/config.
- Dashboard shows coverage by service and consumer.
- Stale evidence is visible and expires predictably.

---

### Phase 4 - Impact-Targeted Artifacts

Goal: make generated tests and release notes part of the core differentiator.

#### Work items

1. Generate tests from diff/change/evidence, not just Jira/spec.
2. Add deterministic test templates per change kind.
3. Add per-consumer migration guides.
4. Add release-note state workflow.
5. Link generated tests in PR comments.
6. Link migration guides by consumer/team.
7. Add review/publish controls in UI.

#### Exit criteria

- For each breaking change, Radar can generate at least one relevant test artifact.
- Release notes include affected consumers and evidence.
- Migration guide is scoped to consumer usage.

---

### Phase 5 - Public Readiness

Goal: publish as a credible enterprise product.

#### Work items

1. Produce polished README and product page.
2. Create demo repository set:
   - producer service
   - two consumer services
   - sample GitHub workflow
   - seeded runtime usage
3. Add screenshots and demo video script.
4. Add benchmark and SLO documentation.
5. Add security and privacy docs.
6. Add licensing review.
7. Add release versioning and changelog.
8. Add self-host install guide.

#### Exit criteria

- Public docs can state the product promise without caveats.
- Demo works from clean clone.
- CI is green.
- Enterprise pilot checklist is complete.

---

## 11. Suggested Repositories for Inspiration and Ethical Harvesting

Use these repositories for patterns, architecture ideas, test fixtures, UX inspiration, and integration practices. Do not copy source code unless the license permits it and attribution/compliance are handled.

| Repository | Use for | Harvestable ideas |
|---|---|---|
| [oasdiff/oasdiff](https://github.com/oasdiff/oasdiff) | OpenAPI breaking-change depth | Rule taxonomy, changelog output, GitHub Action ergonomics, approval workflow ideas. |
| [opticdev/optic](https://github.com/opticdev/optic) | API governance CLI and traffic/spec relationship | Diff UX, forward-only governance, spec accuracy from traffic, multi-file spec handling. |
| [pact-foundation/pact_broker](https://github.com/pact-foundation/pact_broker) | Contract relationship and deploy safety | Consumer/provider matrix, verification history, "can I deploy" semantics, broker-style audit history. |
| [schemathesis/schemathesis](https://github.com/schemathesis/schemathesis) | Generated API tests | Property-based API testing, CI output formats, JUnit reports, operation-level test generation. |
| [open-telemetry/opentelemetry-collector](https://github.com/open-telemetry/opentelemetry-collector) | Runtime usage ingestion | Receiver/processor/exporter architecture, batching, retry, config validation, extension model. |
| [open-telemetry/opentelemetry-collector-contrib](https://github.com/open-telemetry/opentelemetry-collector-contrib) | Enterprise telemetry ecosystem | Contrib component layout, gateway integrations, processor/exporter examples. |
| [actions/toolkit](https://github.com/actions/toolkit) | GitHub Action productization | Inputs/outputs, annotations, logging, masking, action packaging. |
| [actions/javascript-action](https://github.com/actions/javascript-action) | Action starter structure | Test/lint/build/release workflow for a standalone action. |
| [backstage/backstage](https://github.com/backstage/backstage) | Service catalog and ownership | Catalog entity model, ownership metadata, integration expectations for platform teams. |
| [Kong/insomnia](https://github.com/Kong/insomnia) | API client/test UX | Collection/workspace UX ideas, environment management patterns, API testing workflows. |

### License and compliance guidance

Before using any external code:

1. Record repository, license, commit SHA, and copied/adapted files.
2. Prefer concepts and interfaces over copied implementations.
3. Avoid GPL/AGPL code in the product unless legal review approves.
4. Keep third-party snippets out of core product code unless attribution is explicit.
5. Use fixtures and examples only if license allows redistribution.

---

## 12. Competitive Inspiration Map

| Competitor/tool | Radar should learn | Radar should avoid |
|---|---|---|
| oasdiff | Deep deterministic diff rules and clear CI output | Competing only on spec diff depth. |
| Optic | Developer-friendly CLI and governance workflows | Becoming a broad docs/design platform. |
| PactFlow | Consumer/provider relationships and deployment safety | Requiring every consumer to author Pact tests before value appears. |
| Schemathesis | Strong generated test discipline | Treating generated tests as the whole product. |
| Postman | Broad API lifecycle workflow | Competing as a general API workspace. |
| Backstage | Ownership catalog integration | Rebuilding a full developer portal. |
| OpenTelemetry | Enterprise-native runtime collection | Building proprietary telemetry plumbing first. |

---

## 13. Development Plan Seeds

These can be converted into `DEVELOPMENT_PLAN.md` epics.

### Epic D1 - Durable Evidence Model

Acceptance criteria:

- `impact_evidence` table exists.
- Blast-radius computation writes evidence records.
- Blast-radius endpoint returns durable evidence.
- Evidence expiry is configurable.
- Tests cover runtime-only, call-site-only, mixed, and stale evidence.

### Epic D2 - GitHub Action

Acceptance criteria:

- Action accepts base/head spec and service id.
- Action posts markdown PR comment.
- Action emits GitHub annotations.
- Action returns stable outputs.
- Fail-open/fail-closed is tested.
- README shows 15-minute setup.

### Epic D3 - Operation-Aware Scanner

Acceptance criteria:

- TypeScript scanner identifies generated client calls.
- Scanner emits operation and field path.
- Low-confidence matches are marked with reason.
- CLI upload preserves confidence.
- Blast-radius policy can ignore low-confidence static-only evidence.

### Epic D4 - OpenTelemetry Ingest

Acceptance criteria:

- Collector component or adapter can send usage events to Radar.
- Config supports service mapping.
- Batching/retry behavior is documented.
- No payload bodies are stored.
- Demo service generates runtime evidence through config.

### Epic D5 - Impact-Targeted Tests

Acceptance criteria:

- Test generation accepts `diff_id`.
- Tests are generated from changes and evidence.
- Generated artifacts are attached to diff.
- PR comment links generated tests.
- At least five change kinds have deterministic templates.

### Epic D6 - Enterprise Catalog and Ownership

Acceptance criteria:

- Backstage `catalog-info.yaml` importer exists.
- CODEOWNERS fallback exists.
- Ownership appears in blast-radius reports.
- Import sync status appears in UI.
- Manual overrides are supported.

### Epic D7 - Audit and Acknowledgements

Acceptance criteria:

- Policy decisions are persisted.
- Manual acknowledgement is supported.
- Override reason is required.
- Acknowledgements can expire.
- Audit history is visible on diff detail page.

---

## 14. Demo Scenario for Publishable Proof

### 14.1 Repositories

Create three demo repositories or fixture directories:

```text
demo-payments-api
demo-billing-svc
demo-mobile-gateway
```

### 14.2 Scenario

1. `demo-payments-api` exposes `GET /users/{id}`.
2. v1 response contains `user.phone`.
3. `demo-billing-svc` reads `user.phone` daily.
4. `demo-mobile-gateway` has a static call site but no recent runtime usage.
5. Producer PR removes `user.phone`.
6. Radar finds:
   - one high-confidence affected consumer from runtime usage
   - one low-confidence affected consumer from static call site
7. Policy `active_consumers` blocks because high-confidence runtime usage exists.
8. PR comment links:
   - diff detail
   - evidence
   - generated tests
   - migration guide
9. Release notes show:
   - breaking field removal
   - affected teams
   - replacement guidance
   - acknowledgement status

### 14.3 Expected demo output

```text
Verdict: BLOCKED

Breaking change:
- GET /users/{id} -> response.user.phone removed

Affected consumers:
- billing-svc: high confidence, runtime usage observed 2 days ago
- mobile-gateway: low confidence, static call site found

Required action:
- billing-svc owner acknowledgement or compatibility restoration required
```

---

## 15. Metrics and SLOs

### 15.1 Product metrics

| Metric | Target |
|---|---:|
| Time to install in one producer repo | < 15 minutes |
| Time from PR open to Radar verdict | < 60 seconds p95 |
| Blast-radius entries with evidence | 100% |
| False-positive rate for high-confidence evidence | < 5% after pilot tuning |
| PR comments with owner action taken | > 50% of blocked PRs |
| Contract incidents after adoption | Down 30% in pilot team |

### 15.2 Technical SLOs

| SLO | Target |
|---|---:|
| API availability | 99.5% for internal deployment |
| Diff computation | p95 < 10s for specs under 2 MB |
| Blast-radius lookup | p95 < 2s for 1,000 consumers |
| Usage ingest | p95 < 500ms per batch |
| Scanner job | Complete 95% of repos under 15 minutes in nightly scan |

---

## 16. Risks and Mitigations

| Risk | Impact | Mitigation |
|---|---|---|
| Empty evidence makes product look weak | High | Prioritize OTel/gateway ingestion and seeded demo data. |
| Static scanner false positives hurt trust | High | Confidence scoring, operation-aware scanning, policy ignores low-confidence by default. |
| Diff engine lags competitors | Medium | Consider embedding or interoperating with oasdiff for OpenAPI depth rather than reimplementing every rule. |
| Enterprise setup feels heavy | High | GitHub Action, Backstage importer, OIDC guide, docker compose quickstart. |
| LLM-generated artifacts hallucinate | Medium | Keep deterministic data and templates; LLM only writes narrative around verified facts. |
| Payload privacy concerns block adoption | High | Do not require payload capture; store field-path summaries only. |
| Multi-tenant leakage | Critical | Complete org-scoping audit before hosted or multi-tenant deployment. |
| CI fails due Radar outage | Medium | Explicit fail modes; enterprise default closed, team-configurable. |

---

## 17. Documentation Deliverables

To reach publishable status, create:

1. `docs/getting-started-github-action.md`
2. `docs/runtime-usage-ingestion.md`
3. `docs/backstage-integration.md`
4. `docs/policy-reference.md`
5. `docs/evidence-confidence.md`
6. `docs/security-and-privacy.md`
7. `docs/demo-scenario.md`
8. `docs/generated-artifacts.md`
9. `docs/enterprise-deployment.md`

---

## 18. Decision Recommendations

### 18.1 Product decisions

1. Make "impact-aware contract drift" the main product thesis.
2. Treat OpenAPI diffing as required infrastructure, not the differentiator.
3. Prioritize evidence collection over more UI breadth.
4. Prioritize GitHub Action packaging over more ad hoc CLI flags.
5. Tie generated tests and release notes to actual impacted consumers.

### 18.2 Technical decisions

1. Add durable evidence and policy-decision tables.
2. Build OTel/gateway ingestion before more bespoke SDKs.
3. Make scanner confidence explicit.
4. Keep deterministic classification separate from LLM prose.
5. Move toward modular API internals before splitting services.

### 18.3 Go-to-market decisions

1. First publish as self-hosted/internal platform tooling.
2. Target platform engineering and API governance teams.
3. Demo with a realistic three-repo scenario.
4. Position against "diff-only" tools, not against Postman or PactFlow directly.
5. Emphasize auditability, ownership, and consumer evidence.

---

## 19. Source References

- oasdiff: https://github.com/oasdiff/oasdiff
- oasdiff breaking-change rules: https://www.oasdiff.com/docs/breaking-changes
- Optic: https://github.com/opticdev/optic
- Pact Broker: https://github.com/pact-foundation/pact_broker
- Schemathesis: https://github.com/schemathesis/schemathesis
- OpenTelemetry Collector: https://github.com/open-telemetry/opentelemetry-collector
- OpenTelemetry Collector Contrib: https://github.com/open-telemetry/opentelemetry-collector-contrib
- GitHub Actions Toolkit: https://github.com/actions/toolkit
- GitHub JavaScript Action Template: https://github.com/actions/javascript-action
- Backstage: https://github.com/backstage/backstage
- Backstage Software Catalog docs: https://backstage.io/docs/features/software-catalog/
- Kong Insomnia: https://github.com/Kong/insomnia


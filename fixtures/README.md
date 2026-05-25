# Demo Scenario Fixtures

Deterministic fixture set for E-6 integration tests and demo walkthroughs.

## Scenario

**Producer:** `payments-api`  
**Breaking change:** `GET /users/{id}` — `response.body.phone` field removed (v1 → v2)

**Consumers:**

| Consumer | Evidence type | Confidence |
|---|---|---|
| `billing-svc` | runtime_usage (OTel) | high |
| `mobile-gateway` | static_call_site (S2 TypeScript) | medium |

## Files

```
fixtures/
  demo-payments-api/
    v1.yaml                    — OpenAPI v1 (phone field present)
    v2.yaml                    — OpenAPI v2 (phone field removed — breaking)
  demo-billing-svc/
    usage_events.json          — runtime usage evidence for phone field
  demo-mobile-gateway/
    src/clients/users.ts       — TypeScript client accessing response.phone
  expected-pr-comment.md      — structural section markers for comment assertions
  README.md                   — this file
```

## Expected diff result

Running `radar diff v1.yaml v2.yaml` should produce one change:

- **FieldRemoved** · `response.body.phone` · Severity: **Breaking**

## Expected blast radius

With the fixture evidence loaded, `blast_radius` should return:

- `billing-svc` — high confidence (runtime_usage, recent)
- `mobile-gateway` — medium confidence (static_call_site, operation known)

## Expected PR comment sections

1. Summary header
2. Breaking Changes table (1 row: phone removed)
3. Blast Radius table (2 consumers)
4. Evidence table (phone field access, high + medium)
5. Policy Verdict badge (BLOCKED when fail_mode=closed)

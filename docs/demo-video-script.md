# Demo Video Script

**Title:** Radar Monitor — Catching a Breaking API Change Before It Ships  
**Duration:** ~5 minutes  
**Audience:** Platform engineers, API team leads, developers

---

## [0:00] Hook (30 seconds)

**Screen:** PR comment with a Radar verdict — "BLOCKED · 1 breaking change · 2 consumers affected"

> "Your team just opened a pull request that removes a field from your API response.
> Nobody noticed — until Radar did.
> This is Radar Monitor: it diffs your API specs, names the consumers who'll break,
> and blocks the PR until you've notified them.
> Let me show you how it works in five minutes."

---

## [0:30] The breaking change (45 seconds)

**Screen:** Side-by-side diff of v1.yaml and v2.yaml (highlight phone field removal)

> "Here's the diff. Version one has a `phone` field in the response.
> Version two removes it. Small change — massive blast radius if your consumers read it.
>
> Let's run Radar against these two specs."

**Screen:** Terminal — `radar check --base v1.yaml --head v2.yaml`

```
BREAKING  FieldRemoved  GET /users/{id} -> response.body.phone  (severity: breaking)
```

> "One breaking change. Now, who cares? That's what blast radius is for."

---

## [1:15] Blast radius — evidence that matters (60 seconds)

**Screen:** Terminal showing blast radius response / dashboard

> "Radar doesn't just say 'this field changed.' It tells you which services
> actually read that field — and how it knows.
>
> billing-svc: high confidence. We know because its OTel traces show it reading
> `response.body.phone` every day.
>
> mobile-gateway: medium confidence. The tree-sitter scanner found
> `response.phone` on line 14 of `src/clients/users.ts`."

**Screen:** Evidence Coverage page

> "The Evidence Coverage dashboard shows exactly where we have signal and where we have gaps."

---

## [2:15] The policy engine (45 seconds)

**Screen:** `.radar.yml` config

> "You control what happens when a breaking change lands.
> Three modes: closed blocks the PR whenever active consumers are at risk.
> Warn posts a comment but doesn't block.
> Open only blocks on high-confidence evidence.
>
> The default is `closed` — which means this PR is blocked."

**Screen:** PR comment on GitHub

> "The PR comment shows the verdict, the blast radius table, and the evidence.
> The reviewer sees exactly what they're approving."

---

## [3:00] Migration guide and test generation (45 seconds)

**Screen:** `GET /v1/diffs/<id>/migration-guide` response in browser

> "Radar generates a migration guide automatically.
> Per-change migration advice, evidence scoped to each consumer,
> and static call-site locations so consumers know exactly what to fix."

**Screen:** Generated test suite JSON

> "It also generates test stubs — deterministic Postman tests for each breaking change.
> You can run them in Newman against a staging environment to verify consumers
> have actually updated their code."

---

## [3:45] Acknowledging and unblocking (30 seconds)

**Screen:** GitHub PR — label added, rerun shows OVERRIDDEN

> "Once you've notified your consumers and they've confirmed they're ready,
> add the `drift-ack` label to the PR. Radar's verdict flips to OVERRIDDEN — merge is unblocked.
> The decision is logged in the audit trail."

---

## [4:15] Self-host and CI integration (30 seconds)

**Screen:** docker-compose.yml / GitHub Action snippet

> "Radar runs on your own infrastructure — Docker Compose, PostgreSQL, any OIDC provider.
> The GitHub Action wires it into your CI in three lines.
> All state stays in your own database. No data leaves your environment."

---

## [4:45] Closing (15 seconds)

> "That's Radar Monitor: evidence-backed blast radius, policy-gated PRs, and auto-generated
> migration artifacts — all from a single `docker compose up`.
>
> Repo and docs are in the description. Try the demo scenario in under five minutes."

**Screen:** `bash fixtures/seed-demo.sh` running, dashboard loaded

---

## Talking points (for Q&A)

- **"How does it know which consumers are affected?"** — Three evidence sources: OTel runtime traces (high confidence), tree-sitter static scan (medium), Postman collection files (medium). Each source feeds the same `impact_evidence` table.
- **"Does it work with GraphQL?"** — Yes, and protobuf. Same diff engine, same blast radius, same policy.
- **"What if our consumers aren't in the same company?"** — The consumer registry supports any name/team/contact. Add consumer entries manually or import from your Backstage catalog.
- **"What's the operational overhead?"** — One binary (`radar-api`), one database (PostgreSQL). The background job runs hourly. No message queue, no sidecars, no Kubernetes operator required.
- **"Can we run it in CI without a long-running server?"** — Yes: `fail_mode: open` + no `--api-url` gives you a local diff-only check with no server dependency. You lose blast radius but keep the spec diff and exit code.

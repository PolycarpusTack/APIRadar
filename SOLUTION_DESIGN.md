# API Contract Radar Monitor — Solution Design

> **Template version:** 1.0 · Stub: section shape is final, contents are deliberately unfinished. Keep section order stable across the suite.

---

## 0. Metadata

| Field | Value |
|---|---|
| Idea ID | `21-API-Contract-Drift-Monitor` |
| Status | `stub` |
| Owner | Yannick Verrydt |
| Last updated | `2026-04-20` |
| Doc version | `0.1` |
| Classification | Internal |
| Cluster | `Quality` |
| Tech surface | `CLI`, `Service`, `Web` |

---

## 1. TL;DR

A schema-diff tool that closes the loop between "API changed" and "who's going to page me on Monday." Parses OpenAPI, GraphQL, and protobuf; classifies each change as breaking/non-breaking; joins the diff with consumer telemetry and consumer-repo call-site extraction; and posts a PR comment naming the services that will break. Blocks CI when unknown consumers would be hit. Different from openapi-diff or buf-breaking: those answer "is this a break?" — this answers "who breaks, and how bad?"

---

## 2. Problem & Users

### 2.1 Problem
Producer teams in microservice orgs ship breaking API changes because schema-diff tools report changes in the abstract. A removed field looks identical in the diff whether zero consumers read it or twelve do. The cost shows up later as consumer-side incidents, emergency hotfixes, and trust decay between teams. In medium orgs (30-100 services) we've seen 2-5 contract-related incidents per quarter, each averaging 3-6 engineer-hours to unwind plus the producer team's reputational cost.

### 2.2 Primary users
| Persona | Job-to-be-done | Success looks like |
|---|---|---|
| Platform engineer | Stop being blamed for breaks they didn't catch in review | PR comment names the consumer and reviewer reassigns the PR themselves |
| API producer (backend lead) | Ship fast without calling every consumer team | Breaking changes get an auto-generated migration guide that consumers can follow |
| API consumer (downstream team) | Know when upstream is about to break me, before it ships | Subscribe to a producer and get notified at PR-open, not at 2am |
| DevProd / SRE | Prove contract-break incident rate is going down | Dashboard with trend line and incidents-avoided counter |

### 2.3 Non-users / non-goals
- Not for public-API vendors with unknown consumers — we need to see the consumer repos or runtime telemetry.
- Not a schema registry. Integrates with Confluent / Buf registries, does not replace them.
- Does not auto-rewrite consumer code. Drafts migration notes; humans apply them.
- Not a gateway / runtime policy enforcer. Detects at CI time, not at request time.

---

## 3. Solution Overview

### 3.1 Core idea
The interesting number isn't "what changed" — it's "who will break." Every spec diff is joined with two data sources: (a) runtime usage telemetry (which fields/operations are actually called and by whom, over the last N days) and (b) static call-site extraction from known consumer repos (who references this field in code, even if they haven't called it recently). The PR comment becomes a named list, and CI policy can gate on "no active consumers affected" rather than on the change's inherent breaking-ness.

### 3.2 Capabilities
- **Multi-format schema diff** — OpenAPI 3.x, GraphQL SDL, protobuf (proto3). Each with format-specific breaking-change rules.
- **Consumer registry** — producers declare consumers (repo URL + service name + contact); consumers can self-register via a tag in their repo.
- **Usage telemetry ingest** — OTLP-compatible endpoint accepts field-access events; PostgreSQL with 30-90d rolling window.
- **Call-site extraction** — tree-sitter queries per language (TS, Python, Go, Rust, Java) pull references to generated client types out of consumer repos.
- **Blast-radius computation** — for each breaking change, list of (consumer, confidence, last-seen, code-references).
- **Migration guide generator** — per-consumer markdown: what broke, the old usage it found, the suggested new shape.
- **API Release Notes generator** — on each diff run, produces a versioned consumer-facing release-notes document from the classified changes; Markdown template-driven with Claude filling narrative sections; publishable to docs sites or pasted into GitHub Releases.
- **CI integration** — GitHub/GitLab/Bitbucket PR comment + status check; policy file controls block-vs-warn.
- **Dashboard** — cross-service trend view, top-breaking producers, consumer exposure heatmap.
- **Interactive API Playground** — embedded Scalar-powered "Try It" explorer rendered from the already-parsed OpenAPI spec; configurable base URL per environment (production, staging, pre-sales sandbox); no external API client or per-seat licence required.

### 3.3 How it differs from existing tools
| Existing tool | What it does | What we do differently |
|---|---|---|
| openapi-diff | Lists OpenAPI changes with breaking-severity | Also joins with consumer usage and names them |
| buf breaking | Protobuf-native breaking detection | Multi-format + consumer-aware |
| Optic | Learns API from traffic, detects drift vs spec | Focused on spec-vs-spec diff across versions, not traffic-vs-spec |
| Confluent Schema Registry | Enforces compat rules at registration time | Adds consumer-specific blast radius on top of compat class |
| Stoplight Spectral | Lints a single spec | Operates on pairs of specs + cross-repo data |

---

## 4. Architecture

### 4.1 C4 Level 1 (system context)
```
                      ┌───────────────────┐
                      │ Consumer repos    │
                      │ (tree-sitter scan)│
                      └─────────┬─────────┘
                                │
┌──────────────┐      ┌─────────▼──────────────────────┐      ┌─────────────────┐
│ Producer PR  │ ───▶ │ radar-cli (CI runner)           │ ───▶ │ PR comment bot  │
│ (CI runner)  │      │                                 │      │ (GH/GL/BB)      │
└──────────────┘      └─────────┬───────────────────────┘      └─────────────────┘
                                │ HTTP
                      ┌─────────▼──────────────────────┐
                      │ radar-api  (Rust/axum)          │
                      │ SQLite mode  │  PostgreSQL mode  │
                      └──────┬───────────────┬──────────┘
                             │               │
               ┌─────────────▼──┐    ┌───────▼──────────────────┐
               │ radar-desktop  │    │ Browser (web deployment)  │
               │ (Electron)     │    │                           │
               │ radar-ui inside│    │ radar-ui (Vite/React)     │
               │ SQLite on disk │    │ PostgreSQL on server      │
               └────────────────┘    └───────────────────────────┘

                      ┌───────────────────┐      ┌─────────────────┐
                      │ Usage telemetry   │ ◀──── │ Consumer apps   │
                      │ (SQLite / PG)     │      │ (OTLP exporter) │
                      └───────────────────┘      └─────────────────┘
```

### 4.2 Components
| Component | Responsibility | Tech |
|---|---|---|
| `radar-cli` | Local / CI diff runs; prints or posts results | Rust |
| `radar-api` | Consumer registry, usage ingest, diff results store; serves `radar-ui` static assets in web mode | Rust (axum) |
| `radar-scanner` | Worker: scans consumer repos for call sites | Rust + tree-sitter |
| `radar-ui` | Shared renderer — cross-service dashboard, diff viewer, Playground; runs in browser or as Electron renderer | Vite 6 + React 19 + TypeScript + Tailwind |
| `radar-desktop` | Electron shell — wraps `radar-ui`, spawns `radar-api` as a local sidecar, manages SQLite file lifecycle | Electron 33 + electron-vite |
| `drift-db` | Usage events, diffs, consumers, policies | SQLite (local / Electron default) · PostgreSQL 16 (web / production) |

### 4.3 Data flow
1. Producer opens PR; CI runs `radar-cli check --base main --head HEAD`.
2. CLI fetches old + new spec (from repo / registry), parses both, computes typed diff.
3. For each breaking change, CLI calls `radar-api` with the (spec, field-path) pair.
4. `radar-api` queries usage telemetry + latest scanner results for that spec's registered consumers.
5. Blast radius = union of (consumers who called it in last N days) + (consumers with static references).
6. CLI renders PR comment with migration guide per affected consumer.
7. CLI exits non-zero if policy says "block on active consumers affected."
8. Dashboard receives the diff record for trend reporting — result visible in `radar-ui` (browser or Electron).

### 4.4 Deployment topology

Two deployment targets share the same `radar-ui` codebase and the same `radar-api` binary:

**Desktop / local (Electron)**
`radar-desktop` ships as a single installable (`.exe` / `.dmg` / `.AppImage`). On launch it spawns `radar-api` as a child process pointed at a local SQLite file. `radar-ui` loads inside the Electron renderer via `electron-vite`. No external infrastructure required — download and run.

**Web / production**
`radar-api` (PostgreSQL mode) + `drift-db` (PostgreSQL 16) deploy on a VM or container. `radar-api` serves the pre-built `radar-ui` static bundle from `/app`. `radar-scanner` runs as a cron job on the same host or a separate worker. HA deployment viable at scale by adding a read replica and a second API node behind a load balancer.

**CLI (CI)**
`radar-cli` runs in any CI runner independently of both deployment targets. It calls `radar-api` over HTTPS for blast-radius lookups and diff persistence; for air-gapped runs it can operate locally against the SQLite file.

Common to both: `radar-scanner` is always a background cron job, never per-PR (cost control).

---

## 5. Tech Stack

| Layer | Choice | Rationale / Constraint |
|---|---|---|
| Language(s) | Rust 1.80+ for CLI + services; TypeScript 5.x for UI | Fast parsing matters for per-PR runtime; Rust also gives shippable cross-platform binary |
| Backend framework | axum (HTTP API); serves `radar-ui` static assets in web mode | Minimal footprint; single binary for both API and static file serving |
| Frontend | Vite 6 + React 19 + TypeScript + Tailwind + shadcn/ui | Vite works in both browser and Electron renderer; Next.js SSR is incompatible with Electron |
| Desktop shell | Electron 33 + electron-vite | Single installable for Windows / macOS / Linux; same `radar-ui` renderer, no extra runtime |
| State / Data | SQLite (local / Electron default) · PostgreSQL 16 (web / production); same sqlx migrations run on both | sqlx `AnyDatabase` lets `radar-api` compile once and target either engine via `--db sqlite:path` or `--db postgres://…` |
| AI provider(s) | Claude for migration-guide prose and release-notes narrative generation | Deterministic diff stays rules-based; only the human-readable sections use LLM |
| API Playground | Scalar (MIT, self-hosted) embedded in `radar-ui` | Zero per-seat cost vs Postman; renders directly from the OpenAPI spec already parsed for drift; works identically in browser and Electron |
| Packaging | cargo (CLI + services) · electron-builder (desktop installers) · pnpm (UI) · Docker (web self-host) | Single pnpm workspace covers `radar-ui` and `radar-desktop` |
| CI / Release | GitHub Actions | Suite convention |
| Observability | OpenTelemetry → Prometheus / Loki | Suite convention |

---

## 6. Design System Compliance

> UI must use Mediagenix AIR. Source of truth: `../unified-styleguide.html`.

### 6.1 Required tokens
Import `Ideas/_TEMPLATE/design-tokens.css`. Dashboard is dark-first on `--bg-base`. Cobalt `#3805E3` drives primary CTAs (Approve diff, Post comment). Status pills use `--red` for breaking, `--amber` for non-breaking-but-risky, `--teal` for pure additions.

### 6.2 Required patterns
- Dashboard uses `.sg-shell` + 256px `.sg-nav` with sections: Services, Diffs, Consumers, Policies, Settings.
- Diff view is a two-pane monospace layout (`--font-mono` / JetBrains Mono) with token-level highlighting in `--red-dim` / `--green-dim`.
- PR-comment rendering is also dark-first where the host allows; otherwise respects host theme but keeps cobalt accents.
- Live-scan indicator uses `.live-indicator` + `--live-red` pulse only when a scan is in progress.

### 6.3 Required components
- `.data-table` for the consumer blast-radius list (columns: consumer, last-call, code-refs, severity).
- `.pill-err` / `.pill-warn` / `.pill-ok` for change classifications.
- `.kpi-card` for the dashboard tiles: breaking-changes-30d, consumers-at-risk, incidents-avoided.
- `.btn-primary` cobalt for "Acknowledge" and "Draft migration PR"; `.btn-danger` for "Override block."

### 6.4 Do / Don't
| Do | Don't |
|---|---|
| Reserve `--red` for breaking-change severity | Use `--red` for the "delete" button chrome |
| Use monospace for field paths (`user.phone.number`) | Use monospace for the dashboard body copy |
| Use `--cobalt` for primary CTA | Use cobalt AND neon-green as co-equal CTAs in the diff view |
| Glow (`--glow-cobalt`) on hover for actionable diff rows | Glow on every row |

### 6.5 CLI / terminal output
- Breaking changes: red. Non-breaking risky: amber. Safe: teal. Section headers: cobalt.
- Blast radius rendered as a box-drawn table; consumer names in default text, last-seen times in `--text-dim`.
- Respects `NO_COLOR` and `--no-color`.

---

## 7. Interfaces

### 7.1 CLI
```
drift <command> [options]

Commands:
  check           Diff two specs and compute blast radius
  register        Register this repo as a consumer of a producer
  scan            Scan a consumer repo for call sites (admin)
  explain         Expand a single diff into a migration guide

Common options:
  --base REF              Git ref for baseline spec
  --head REF              Git ref for candidate spec
  --spec PATH             Spec file(s); glob allowed
  --format openapi|graphql|proto
  --policy FILE           Policy overrides (default .radar.yml)
  --post-comment          Post to current PR
  --json                  Machine-readable output
  --no-color              Disable colour (also NO_COLOR)
```

### 7.2 HTTP API
```
POST /v1/usage/events            Ingest field-access events (OTLP-like)
GET  /v1/services/:id/diffs      List diffs for a producer
POST /v1/services/:id/diffs      Submit a diff from CI
GET  /v1/services/:id/consumers  List registered consumers
POST /v1/consumers               Self-register as consumer
GET  /v1/diffs/:id/blast-radius  Compute/retrieve blast radius
```
OpenAPI spec shipped at `docs/openapi.yaml`.

### 7.3 Config schema
```yaml
# .radar.yml
version: 1
service: billing-api
specs:
  - path: openapi/billing.yaml
    format: openapi
policy:
  block_on: active_consumers      # active_consumers | any_break | never
  lookback_days: 30
  allow_override_with: label:drift-ack
consumers:
  registry_url: https://drift.internal/v1
```

### 7.4 API Release Notes template

`drift explain --release-notes` renders the following Markdown template, with Claude filling the narrative sections from the classified diff:

```markdown
# Release Notes — {service.name} {to_version}

_Generated {date} · Diff #{diff.id} · {from_version} → {to_version}_

## ⚠️ Breaking Changes

| Field / Operation | Change | Affected consumers |
|---|---|---|
| `{field_path}` | {change_description} | {consumer_list} |

> {claude_narrative: explain the impact in plain language and the recommended migration path}

## ✅ New Capabilities

| Field / Operation | Description |
|---|---|
| `{field_path}` | {change_description} |

## 🔔 Deprecations

| Field / Operation | Sunset date | Replacement |
|---|---|---|
| `{field_path}` | {sunset_date} | `{replacement_path}` |

## 📋 Per-Consumer Migration Checklist

### {consumer.name}

- Detected call site: `{file}:{line}`
- Old shape: `{old_snippet}`
- New shape: `{new_snippet}`
- Action required: {claude_narrative: specific one-liner for this consumer}

---
_Auto-generated by radar-cli {cli_version}. Review before publishing._
```

**Output options:**
- `--release-notes` — prints Markdown to stdout
- `--release-notes --post-github-release` — creates or updates the GitHub Release on `to_version`
- `--release-notes --out FILE` — writes to file

### 7.5 Interactive API Playground

Embedded Scalar instance in the service detail view of `drift-dashboard`. Designed for pre-sales sandbox demos and developer testing — no Postman or external API client required.

| Setting | Detail |
|---|---|
| Spec source | Same OpenAPI YAML already stored in `spec_version.spec_blob`; no re-fetch |
| Base URL | Configurable per named environment (`production`, `staging`, `sandbox`) via the service's env config in the dashboard |
| Auth | Bearer token / API key injected from the environment config; never stored in browser |
| Request history | Local browser storage only; never sent to `radar-api` |
| Export | One-click copy as `curl`, `fetch`, or language snippet (Python / Go / Rust) |
| Branding | Scalar's default UI skinned with design-system tokens (`--bg-base`, `--cobalt` primary) |

The playground tab is visible on every service detail page. For pre-sales demos, the sandbox environment is pre-configured at deploy time so the demo operator only needs to open the browser — connectivity to the target Base environment is shown live without any additional tooling cost.

---

## 8. Data Model

### 8.1 Entities
| Entity | Fields | Notes |
|---|---|---|
| `service` | `id, name, repo_url, owner_team, spec_format` | The producer |
| `spec_version` | `id, service_id, git_ref, captured_at, spec_blob` | Immutable snapshot |
| `diff` | `id, from_version, to_version, pr_url, created_at, summary` | One per check run |
| `change` | `id, diff_id, path, kind, severity` | Individual diff entry |
| `consumer` | `id, name, repo_url, owner_team, contact` | Consuming service |
| `subscription` | `id, service_id, consumer_id, opted_in_at` | Consumer registered to producer |
| `usage_event` | `consumer_id, service_id, operation, field_path, ts` | Hot path; hypertable |
| `call_site` | `consumer_id, service_id, field_path, file, line, scanned_at` | From tree-sitter scan |

### 8.2 Storage
PostgreSQL 16. `usage_event` can upgrade to TimescaleDB hypertable at scale; default retention 90 days. Spec blobs are compressed and deduplicated by content hash.

### 8.3 Migrations
`sqlx migrate`. Migrations are version-pinned and run by the service on startup behind a feature flag; destructive migrations require explicit env var.

---

## 9. Security & Privacy

- **Threat model:** (1) malicious spec input (billion-laughs-style in OpenAPI refs) — mitigated: 4 MB body limit (`DefaultBodyLimit::max`) + axum JSON extractor depth. (2) consumer registry spoofing — not yet mitigated: repo-ownership proof (file on default branch) is on the roadmap. (3) usage-event flood — mitigated: per-IP sliding-window rate limiter (default 300 req/min), plus per-batch size cap.
- **Rate limiter trust-proxy caveat:** client IP is read from `X-Forwarded-For` (first value). Clients can spoof this to bypass rate limiting unless the reverse proxy overwrites the header (`proxy_set_header X-Forwarded-For $remote_addr`). Document this in the runbook; do not rely on rate limiting as the sole flood defence.
- **Secrets handling:** service tokens in env vars; never logged. Sandbox-environment bearer tokens masked to last-4-chars in all API responses.
- **AuthN / AuthZ:** static bearer token (`RADAR_SERVICE_TOKEN`) or HS256 JWT (`RADAR_JWT_SECRET`) with `org_id` claim for row-level tenancy. `/health` and `/metrics` are unauthenticated by design. OIDC not yet implemented.
- **PII:** none expected. Field paths are schema-only, not field values.
- **Data leaves the machine?** Yes, per design — usage telemetry and diffs are sent to `radar-api`. Self-hosted by default. AI endpoints call Anthropic/OpenAI APIs.
- **Supply chain:** `Cargo.lock` + `pnpm-lock.yaml` committed; `cargo audit` in CI (RUSTSEC-2023-0071 acknowledged — transitive mysql dep, no mysql usage); CycloneDX SBOM generated on main-branch releases.

---

## 10. Observability

| Signal | Emitted by | Where it lands |
|---|---|---|
| Logs | `tracing` (Rust), `pino` (Node) | stdout -> Loki |
| Metrics | OTLP | Prometheus |
| Traces | OTLP | Tempo |
| User events | radar-cli + dashboard | internal events table |

Key SLIs: `check` p95 < 5s for specs under 1 MB; blast-radius query p95 < 300ms; false-positive rate on "breaking" below 2%.

---

## 11. Roadmap

| Phase | Weeks | Theme | Exit criteria |
|---|---|---|---|
| P0 | 1-3 | OpenAPI diff CLI + trivial PR comment, one producer; `radar-ui` + `radar-desktop` Electron shell (SQLite) | `drift check` runs in CI; Electron app launches and connects to local radar-api sidecar |
| P1 | 4-7 | Consumer registry + usage ingest + blast radius on OpenAPI; release-notes template + `--release-notes` flag | Three consumers registered; PR comment names them; release notes generated from a real diff |
| P2 | 8-10 | GraphQL + protobuf; tree-sitter scanner; full `radar-ui` dashboard + Scalar Playground; PostgreSQL mode for web deployment | Multi-format; static call-site refs in blast radius; pre-sales sandbox demo runnable from browser or Electron |
| P3 | 11-14 | Migration-guide generator; policy engine; multi-org OIDC; release-notes → GitHub Release automation; signed installers + Docker image | Self-service onboarding; SaaS-viable web deploy; installable desktop app on all three platforms |

---

## 12. Risks

| Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|
| Consumers don't emit usage telemetry | H | H | Tree-sitter static scan gives a baseline even without runtime data |
| False positives erode trust in CI block | M | H | Default to warn-only; block is opt-in per repo |
| Schema parsers lag real-world specs (vendor extensions) | M | M | Pluggable parser layer; fall back to "unknown -> warn" |
| Consumer-repo scan hits rate limits / perms | M | M | Run scanner with a dedicated app token; cache aggressively |
| Adoption requires coordinated consumer buy-in | H | M | Single-producer-single-consumer value demoable; no org-wide rollout required |

---

## 13. Cost & ROI

### 13.1 Build cost
2 engineers x 14 weeks for P0-P3; part-time design for dashboard. Estimate: ~$180-220k loaded.

### 13.2 Run cost
Self-host: <$50/mo for small orgs (one Postgres, one small VM). SaaS at 10k services: dominated by scanner throughput + telemetry ingest; estimate $3-6/service/month.

### 13.3 Value / pricing hypothesis
OSS core (CLI + single-repo use). Commercial tier for cross-service dashboard, consumer registry, and migration-guide generation. Pricing per producer service.

---

## 14. Success Metrics

| Metric | Baseline | Target (P1) | Target (P3) |
|---|---|---|---|
| Contract-break incidents per quarter | TBD from dogfood | -30% | -70% |
| PRs with blast radius comment | 0 | 50% of producer PRs | 95% |
| Mean time to migration after breaking change | unknown | <2 weeks | <3 days |
| False-positive rate on "breaking" classifications | — | <5% | <2% |

---

## 15. Open Questions

- [ ] GraphQL: is schema-level diff enough, or do we need persisted-query diff too?
- [ ] How to handle specs that live in a separate registry (Confluent, Buf) vs in-repo?
- [ ] Is the consumer-opt-in model workable, or do we need producer-side consumer inference from logs?
- [ ] How far to push migration-guide generation before it becomes a liability (wrong suggestions)?

---

## 16. Related Ideas in this Suite

- `09-Git-Repo-Health-Checker` — shares cross-repo scanning infrastructure and dashboard shell
- `24-Dependency-Lifecycle-Dashboard` — same "track producers and consumers" mental model, different domain
- `05-Code-Review-Agent` — shares the PR-comment surface; they co-review the same PRs

---

## 17. Changelog

| Version | Date | Author | Change |
|---|---|---|---|
| 0.1 | 2026-04-20 | Yannick Verrydt | Initial stub |
| 0.2 | 2026-05-17 | Yannick Verrydt | Added API Release Notes generator (section 3.2, 7.4); added Interactive API Playground via Scalar for pre-sales sandbox demos (section 3.2, 7.5); updated tech stack and roadmap |
| 0.3 | 2026-05-17 | Yannick Verrydt | Adopted Electron + Web dual-deployment: replaced Next.js with Vite 6; introduced `radar-ui` (shared renderer) and `radar-desktop` (Electron shell); added SQLite as local/default DB with PostgreSQL as production option; updated C4 diagram, components, deployment topology, tech stack, and roadmap |

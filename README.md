# 21-API-Contract-Drift-Monitor

> **Status:** `stub` · **Cluster:** `Quality` · **Surface:** `CLI + CI + Web`

Diffs OpenAPI / GraphQL / protobuf across versions, names the consumers each change will break, and blocks PRs that cross the line.

## Why it matters
API producers ship breaking changes by accident because they can't see who's still calling the old shape. Existing tools (openapi-diff, buf breaking) detect the diff but stop there — nobody tells the reviewer "this removes `user.phone`, which `billing-svc` and `mobile-ios` read daily." This closes the loop between schema diff and consumer telemetry so PRs carry a named blast radius.

## Shape
- **Primary user:** Platform teams and API producers in orgs with 10+ consumer services
- **Key capability:** Blast-radius-aware schema diff: every breaking change lists the consumers it will break, by name, with last-seen timestamps
- **Key differentiator:** Joins schema diff with actual call-site usage from consumer repos and runtime telemetry — not static analysis of the spec alone
- **Tech cluster:** Rust CLI + PostgreSQL + React dashboard, tree-sitter for consumer call-site extraction

## Status checklist
- [ ] Problem validated with >=3 users
- [ ] `SOLUTION_DESIGN.md` fleshed out
- [ ] Design System Compliance section complete
- [ ] Walking skeleton running
- [ ] First public / internal release

## Docs
- [`SOLUTION_DESIGN.md`](./SOLUTION_DESIGN.md) — full spec
- [`DEVELOPMENT_PLAN.md`](./DEVELOPMENT_PLAN.md) — execution plan _(optional until P0 starts)_

## Related
- `09-Git-Repo-Health-Checker` — shares the "scan many repos, roll up to a dashboard" pattern
- `24-Dependency-Lifecycle-Dashboard` — complementary: that tracks libs, this tracks API contracts
- `05-Code-Review-Agent` — PR-comment surface is shared; both annotate diffs with context a human missed

# Policy Reference

`.radar.yml` controls when radar-action blocks a PR and how it behaves when the Radar API is unreachable. Place it at the root of your producer repository.

## Full configuration reference

```yaml
version: 1

# Which service spec this repo owns (matches the service ID in radar-api)
service: my-payments-api

# Policy: when to block a PR
policy:
  # never | any_break (default) | active_consumers
  block_on: any_break

  # Evidence older than this is ignored for blast-radius decisions
  lookback_days: 30

  # GitHub label that, when present on the PR, overrides a block verdict.
  # Producers can add this label to acknowledge the breaking change.
  allow_override_with: "label:drift-ack"

# Behavior when the Radar API is unreachable:
# closed (default): treat API unreachable as a block
# open:            fall back to local diff only; warn but don't block on API failure
# warn:            never block the build regardless of breaking changes
fail_mode: closed

# Postman / NativeREST collection files to scan automatically (glob patterns)
collection_paths:
  - "**/*.postman_collection.json"
  - "**/*.nativerest_collection.json"
```

## `policy.block_on`

> **Default changed:** `block_on` now defaults to `any_break`. It previously
> defaulted to `active_consumers`, which meant that on a fresh install — where
> nobody has instrumented anything yet — a genuine breaking change exited 0 and
> the check went green. Radar was at its most permissive exactly when a team had
> the least protection, and it reported success while doing it. Teams with real
> evidence coverage can still opt into `active_consumers`; it is now a setting
> you choose once instrumentation exists, rather than the starting point.


| Value | Behavior |
|---|---|
| `never` | Exit 0 always. Breaking changes are surfaced in the PR comment but never block CI. |
| `any_break` | Exit 1 if any Breaking Change is detected, regardless of consumer evidence. |
| `active_consumers` | Exit 1 if an active consumer has evidence for the affected field within `lookback_days`. If the blast radius is empty **and the service has no evidence at all**, exit 1 with verdict `insufficient coverage` — an empty blast radius is only a pass when there is evidence to make it meaningful. |

## `fail_mode`

| Value | Radar API unreachable | Breaking change found |
|---|---|---|
| `closed` (default) | Exit 1 — conservative block | Exit 1 if `block_on` rules trigger |
| `open` | Exit 0 — warn and continue | Exit 1 if `block_on` rules trigger (local diff only, no blast-radius) |
| `warn` | Exit 0 — warn and continue | Exit 0 always — warning logged |

`fail_mode` is independent of `block_on`. For example, `fail_mode: open` with `block_on: active_consumers` means: use local diff to find breaking changes, but don't fail on API errors; block only if we have blast-radius evidence.

## `policy.allow_override_with`

```yaml
policy:
  allow_override_with: "label:drift-ack"
```

When the PR carries the `drift-ack` GitHub label, the policy verdict becomes `overridden` instead of `block`. The label check happens server-side via the GitHub API using `GITHUB_TOKEN`.

**Label-based override workflow:**

1. CI runs → `block` verdict (PR comment shows BLOCKED)
2. Producer or platform team reviews evidence
3. Add `drift-ack` label to the PR
4. Re-run CI → `overridden` verdict → PR comment shows OVERRIDDEN

## Server-side acknowledgements

Beyond labels, you can create a permanent acknowledgement record via the API:

```sh
curl -X POST https://radar.example.com/v1/acknowledgements \
  -H "Authorization: Bearer $RADAR_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "diff_id": "<diff-id-from-pr-comment>",
    "acknowledged_by": "alice@example.com",
    "reason": "All consumers have migrated to v2 of the endpoint",
    "expires_at": "2026-06-30T00:00:00Z"
  }'
```

Acknowledgements take effect on the next CI run without requiring the `drift-ack` label.

**Acknowledgement fields:**

| Field | Required | Description |
|---|---|---|
| `acknowledged_by` | yes | Identifier of the person or system acknowledging (email, username) |
| `diff_id` | no | Specific diff to acknowledge; leave null for service-wide acknowledgement |
| `change_id` | no | Specific change within the diff |
| `consumer_id` | no | Consumer whose impact is being acknowledged |
| `service_id` | no | Producer service |
| `reason` | no | Free-text rationale for audit trail |
| `expires_at` | no | ISO 8601 UTC; after this time the acknowledgement is ignored |

## Policy decisions table

Every CI run writes a `policy_decision` record to radar-api for audit purposes:

```sh
curl https://radar.example.com/v1/policy-decisions?service_id=<id> \
  -H "Authorization: Bearer $RADAR_TOKEN"
```

Each record captures: `verdict`, `fail_mode`, `actor` (e.g. `radar-cli` or `radar-action`), `diff_id`, and `created_at`.

## Evidence and confidence

Policy decisions based on `block_on: active_consumers` use the following confidence rules:

| Source | Confidence | Lookback window |
|---|---|---|
| OTel runtime telemetry (`runtime_usage`) | **high** (< 7 days) / medium | `lookback_days` |
| Tree-sitter static scan (`static_call_site`) | **medium** (operation known) / low | No window (static) |
| Postman collection file (`collection_file`) | **medium** | No window (static) |

`closed` mode blocks when at least one `high` or `medium` confidence evidence record exists for a consumer that uses the changed field. `low` confidence alone does not trigger a block by default.

## Default values

All fields are optional. When `.radar.yml` is absent entirely, the defaults are equivalent to:

```yaml
version: 1
policy:
  block_on: active_consumers
  lookback_days: 30
fail_mode: closed
```

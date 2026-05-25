# Generated Artifacts: Test Suites, Migration Guides, Release Notes

Radar generates three kinds of artifacts from a diff. All three are scoped to a specific diff ID and optionally to a consumer ID.

## Test suites

**Route:** `POST /v1/generate-tests`  
**Route (list):** `GET /v1/diffs/:id/test-suites`

### How they're generated

When `use_templates: true`, Radar generates deterministic Postman-compatible test stubs for each breaking change — no AI required. When an `ANTHROPIC_API_KEY` or `OPENAI_API_KEY` is present, the AI path adds context-aware tests beyond the template.

### Templates by change kind

| `change_kind` | What is generated |
|---|---|
| `field_removed` | Negative test: assert the field is absent; warn consumers to stop reading it |
| `required_changed` | Negative test: send a request omitting the now-required field; assert 400/422 |
| `enum_value_removed` | Negative test: send the removed enum value; assert 400/422 |
| `operation_removed` | Negative test: call the removed endpoint; assert 404 |
| `type_changed` / `nullability_changed` | Smoke test: verify the field is present and matches the new type |

### Consuming test suites in CI

```sh
# Generate tests for a diff
curl -s -X POST http://localhost:8080/v1/generate-tests \
  -H 'Content-Type: application/json' \
  -d '{"diff_id": "<id>", "use_templates": true, "consumer_id": "<id>"}' \
  | jq -r '.collection_json' > generated-tests.postman_collection.json

# Run with Newman
npx newman run generated-tests.postman_collection.json \
  --environment staging.postman_environment.json
```

### Linking to a PR

`drift check --post-github-comment` fetches test suites via `GET /v1/diffs/:id/test-suites` and includes a "Generated Test Suites" section in the PR comment, listing each suite's name, test count, and ID.

---

## Migration guides

**Route:** `GET /v1/diffs/:id/migration-guide`  
**Query params:** `?consumer_id=<id>` (optional — scopes evidence to one consumer)

### What the guide contains

A Markdown document with:

1. **Summary header** — service name, from/to git refs
2. **Breaking changes table** — one row per breaking change with change kind and path
3. **Migration advice** — per-change-kind guidance:
   - `field_removed` — "Update response parsing to no longer expect this field"
   - `required_changed` — "Ensure all request builders include this field"
   - `enum_value_removed` — "Remove handling of the removed enum value"
   - `operation_removed` — "Replace calls to this endpoint with the nearest alternative"
   - Others — "Review the changed contract and update integration tests"
4. **Evidence table** — which consumers accessed which fields, with confidence levels and timestamps
5. **Call-site table** — file paths and line numbers from the static scanner

### Example

```sh
# Full guide for a diff
curl http://localhost:8080/v1/diffs/<id>/migration-guide

# Scoped to one consumer
curl "http://localhost:8080/v1/diffs/<id>/migration-guide?consumer_id=<consumer-id>"
```

---

## Release notes

**Route:** `GET /v1/diffs/:id/release-notes`  
**Status transitions:** `PATCH /v1/release-notes/:id/status`

### State machine

```
draft  -->  reviewed  -->  published  -->  superseded
              |
              v
           draft (revert)
```

| Transition | Allowed? |
|---|---|
| draft → reviewed | Yes |
| reviewed → published | Yes |
| reviewed → draft | Yes (revert) |
| published → superseded | Yes |
| published → reviewed | No |
| superseded → any | No |

### Workflow

1. Radar auto-generates a draft release note when a diff is created (if `ANTHROPIC_API_KEY` is set).
2. The API team reviews it in the **Release Notes** UI tab (or via API).
3. Click "Mark Reviewed" → `reviewed`.
4. Click "Publish" → `published`. The note is now visible to consumers.
5. When a new version supersedes this diff, transition to `superseded`.

### UI

The **Release Notes** page (`/app/release-notes`) shows all notes with status badges and transition buttons. Expanding a card shows the full Markdown content and the available next-status actions.

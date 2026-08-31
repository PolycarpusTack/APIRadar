#!/usr/bin/env bash
# seed-demo.sh — seed a running Radar instance with the demo scenario data.
#
# Usage:
#   RADAR_URL=http://localhost:8080 RADAR_TOKEN=<token> bash fixtures/seed-demo.sh
#
# What it does:
#   1. Registers the payments-api producer service
#   2. Posts v1 and v2 OpenAPI specs; triggers the diff (phone field removed)
#   3. Registers billing-svc and mobile-gateway consumers
#   4. Seeds runtime usage evidence for billing-svc (high confidence)
#   5. Seeds static call-site evidence for mobile-gateway (medium confidence)
#
# After seeding, open the dashboard and navigate to the payments-api diff to
# see the blast radius, evidence table, and policy verdict.

set -euo pipefail

RADAR_URL="${RADAR_URL:-http://localhost:8080}"
RADAR_TOKEN="${RADAR_TOKEN:-}"

auth_header() {
  if [ -n "$RADAR_TOKEN" ]; then
    echo "-H" "Authorization: Bearer $RADAR_TOKEN"
  fi
}

api() {
  local method="$1"; shift
  local path="$1"; shift
  curl -s -X "$method" "$RADAR_URL/v1$path" \
    -H "Content-Type: application/json" \
    $(auth_header) \
    "$@"
}

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

echo "==> Radar demo seeder — targeting $RADAR_URL"

# ── 1. Register payments-api service ────────────────────────────────────────
echo ""
echo "1. Registering payments-api producer service..."
api POST /services -d '{
  "id": "payments-api",
  "name": "Payments API",
  "repo_url": "https://github.com/example/payments-api",
  "owner_team": "platform",
  "spec_format": "openapi"
}' | jq -r '.id // "already exists"'

# ── 2. Post v1 spec ──────────────────────────────────────────────────────────
echo ""
echo "2. Posting v1 spec (git_ref=v1.0.0)..."
V1_YAML=$(cat "$SCRIPT_DIR/demo-payments-api/v1.yaml")
api POST "/services/payments-api/spec-versions" -d "{
  \"git_ref\": \"v1.0.0\",
  \"spec_yaml\": $(jq -Rs . <<< "$V1_YAML")
}" | jq -r '.id // .error'

# ── 3. Post v2 spec + trigger diff ──────────────────────────────────────────
echo ""
echo "3. Posting v2 spec (git_ref=v2.0.0) — triggers diff..."
V2_YAML=$(cat "$SCRIPT_DIR/demo-payments-api/v2.yaml")
DIFF_RESP=$(api POST "/services/payments-api/spec-versions" -d "{
  \"git_ref\": \"v2.0.0\",
  \"spec_yaml\": $(jq -Rs . <<< "$V2_YAML"),
  \"from_ref\": \"v1.0.0\"
}")
echo "$DIFF_RESP" | jq -r '.diff_id // .id // .error'
DIFF_ID=$(echo "$DIFF_RESP" | jq -r '.diff_id // empty')

# ── 4. Register consumers ────────────────────────────────────────────────────
echo ""
echo "4. Registering consumers..."
# Capture the ids: the evidence endpoints below key on consumer_id, not name.
BILLING_ID=$(api POST /consumers/upsert -d '{
  "name": "billing-svc",
  "owner_team": "billing",
  "contact": "billing@example.com"
}' | jq -r '.id // empty')
echo "   billing-svc: ${BILLING_ID:-FAILED}"

MOBILE_ID=$(api POST /consumers/upsert -d '{
  "name": "mobile-gateway",
  "owner_team": "mobile",
  "contact": "mobile@example.com"
}' | jq -r '.id // empty')
echo "   mobile-gateway: ${MOBILE_ID:-FAILED}"

if [ -z "$BILLING_ID" ] || [ -z "$MOBILE_ID" ]; then
  echo "ERROR: consumer registration failed — cannot seed evidence." >&2
  exit 1
fi

# ── 5. Seed runtime usage evidence (billing-svc) ─────────────────────────────
echo ""
echo "5. Seeding runtime usage evidence for billing-svc..."
NOW="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
api POST /usage/events -d "[
  {
    \"consumer_id\": \"$BILLING_ID\",
    \"service_id\": \"payments-api\",
    \"operation\": \"GET /users/{id}\",
    \"field_path\": \"response.body.phone\",
    \"observed_at\": \"$NOW\"
  },
  {
    \"consumer_id\": \"$BILLING_ID\",
    \"service_id\": \"payments-api\",
    \"operation\": \"GET /users/{id}\",
    \"field_path\": \"response.body.email\",
    \"observed_at\": \"$NOW\"
  }
]" | jq -r '.accepted // .error'

# ── 6. Seed static call-site evidence (mobile-gateway) ───────────────────────
echo ""
echo "6. Seeding static call-site evidence for mobile-gateway..."
api POST /call-sites -d "[
  {
    \"consumer_id\": \"$MOBILE_ID\",
    \"service_id\": \"payments-api\",
    \"operation\": \"GET /users/{id}\",
    \"field_path\": \"response.phone\",
    \"file_path\": \"src/clients/users.ts\",
    \"line_number\": 14
  }
]" | jq -r '.upserted // .error'

# ── Summary ──────────────────────────────────────────────────────────────────
echo ""
echo "==> Seed complete."
echo ""
echo "Dashboard: $RADAR_URL/app/"
if [ -n "$DIFF_ID" ]; then
  echo "Blast radius: $RADAR_URL/v1/diffs/$DIFF_ID/blast-radius"
  echo "Migration guide: $RADAR_URL/v1/diffs/$DIFF_ID/migration-guide"
fi
echo ""
echo "CLI verification:"
echo "  radar check --base v1.0.0 --head v2.0.0 --service payments-api \\"
echo "    --api-url $RADAR_URL"

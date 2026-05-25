# Runtime Usage Ingestion

Radar Monitor collects runtime evidence of API field and operation usage from live traffic.
This evidence directly feeds the Blast Radius calculation — a consumer that has been seen
calling a field in the last 30 days gets a higher confidence score than one only spotted
in a static scan.

---

## Ingestion paths

| Path | Protocol | Best for |
|------|----------|----------|
| `POST /v1/usage/events` | JSON (direct) | Custom or embedded instrumentation |
| `POST /v1/otlp/v1/traces` | OTLP JSON (OpenTelemetry) | Services already exporting traces |
| `POST /v1/gateway/logs` | JSON array | API gateway log forwarding |
| SDK middleware | Auto-wired | Node.js / Python services |

All paths write to the same `impact_evidence` table with `source_type = runtime_usage`.

---

## Direct event API

Send a batch of usage events:

```http
POST /v1/usage/events
Content-Type: application/json
Authorization: Bearer <token>

[
  {
    "consumer_id": "billing-svc",
    "service_id":  "payments-api",
    "operation":   "GET /v1/charges/{id}",
    "field_path":  "charge.amount"
  }
]
```

| Field | Required | Description |
|-------|----------|-------------|
| `consumer_id` | Yes | ID of the calling service in Radar's registry |
| `service_id` | Yes | ID of the upstream producer being called |
| `operation` | Yes | HTTP method + route pattern, e.g. `GET /orders/{id}` |
| `field_path` | No | Dot-separated path to the field accessed, e.g. `order.items[].price` |

### Response

```json
{ "accepted": 5 }
```

Events may be silently dropped when:
- The field_path matches the service's `field_deny_list` sampling configuration
- The event is outside the probabilistic `sample_rate` for that service
- The server is under back-pressure (returns 503)

---

## OTLP trace receiver

Services already exporting OpenTelemetry traces can point their OTLP HTTP exporter at
Radar Monitor instead of (or in addition to) their telemetry backend.

### Exporter configuration (OTEL SDK)

```yaml
# opentelemetry-collector.yaml
exporters:
  otlphttp/radar:
    endpoint: http://radar-api:8080/v1/otlp
    headers:
      Authorization: "Bearer ${RADAR_TOKEN}"
```

Or configure the SDK directly:

```python
# Python
from opentelemetry.exporter.otlp.proto.http.trace_exporter import OTLPSpanExporter
exporter = OTLPSpanExporter(
    endpoint="http://radar-api:8080/v1/otlp/v1/traces",
    headers={"Authorization": f"Bearer {token}"},
)
```

### What Radar Monitor extracts

Radar Monitor processes **CLIENT spans** (span kind = 3) only. It reads the following
span attributes:

| Attribute | Usage |
|-----------|-------|
| `http.method` | Part of the operation string |
| `http.route` | Route pattern; falls back to `http.url` path segment |
| `http.target` | Fallback URL when `http.route` is absent |
| `radar.consumer_id` | Explicit consumer override |
| `radar.service_id` | Explicit service override |

When `radar.consumer_id` / `radar.service_id` are absent, Radar falls back to looking up
the consumer by the `service.name` resource attribute in the database.

Numeric path segments are normalised automatically:
`/users/12345` → `/users/{id}`

---

## Gateway log forwarding

Forward a batch of API gateway access log entries:

```http
POST /v1/gateway/logs
Content-Type: application/json
Authorization: Bearer <token>

[
  {
    "method":      "DELETE",
    "path":        "/v1/orders/99",
    "consumer_id": "billing-svc",
    "service_id":  "payments-api",
    "status_code": 204
  }
]
```

All numeric path segments are normalised (e.g. `/v1/orders/99` → `/v1/orders/{id}`).

---

## SDK middleware

The fastest path to evidence collection — add one line to your application.

### Node.js / Express

```bash
npm install @radar-monitor/sdk
```

```js
const { expressMiddleware } = require('@radar-monitor/sdk')
app.use(expressMiddleware({
  radarUrl:   'http://radar-api:8080',
  consumerId: 'billing-svc',
  serviceId:  'payments-api',
  token:      process.env.RADAR_TOKEN,   // optional
}))
```

The middleware fires after each response finishes (`res.on('finish')`). It uses the
matched route pattern (`req.route.path`) where available, so `/users/123` is recorded
as `GET /users/:id`.

Events are batched in-process and flushed every 5 seconds (configurable via
`flushIntervalMs`). The timer is unref'd so it never prevents process exit.

### Python / FastAPI (or any ASGI framework)

```bash
pip install radar-monitor-sdk
```

```python
from fastapi import FastAPI
from radar_monitor import RadarMiddleware

app = FastAPI()
app.add_middleware(
    RadarMiddleware,
    radar_url="http://radar-api:8080",
    consumer_id="billing-svc",
    service_id="payments-api",
    token=os.environ.get("RADAR_TOKEN"),
)
```

Health check paths (`/health`, `/metrics`, `/readyz`, `/livez`) are excluded by default.
Override with `exclude_paths=("/my-health",)`.

### Field-level usage

To record which response fields your code actually reads:

```python
from radar_monitor import RadarBatcher

batcher = RadarBatcher(radar_url=..., consumer_id=..., service_id=...)
# After parsing a response:
batcher.push("GET /orders/{id}", "order.items[].price")
```

```js
const { recordFieldUsage } = require('@radar-monitor/sdk')
recordFieldUsage(batcher, 'GET /orders/{id}', 'order.items[].price')
```

---

## Sampling controls

Radar Monitor supports per-service sampling to reduce noise in high-traffic environments.

### Configure sampling

```http
PUT /v1/services/{service_id}/sampling
Content-Type: application/json
Authorization: Bearer <token>

{
  "sample_rate": 0.1,
  "field_deny_list": ["user.password_hash", "*.internal_*", "auth.**"]
}
```

| Field | Default | Description |
|-------|---------|-------------|
| `sample_rate` | `1.0` | Fraction of events to accept (0.0 – 1.0). `1.0` = keep all. |
| `field_deny_list` | `[]` | Glob patterns for `field_path` values to drop. `*` matches one segment, `**` matches any depth. |

### Retrieve current sampling config

```http
GET /v1/services/{service_id}/sampling
```

---

## Evidence coverage dashboard

The web dashboard at `/evidence-coverage` shows aggregated coverage per
consumer × service × source_type. Rows that have not received new evidence in 7 days are
flagged as stale.

Use this page to verify that your SDKs are correctly deployed and reaching the Radar API.

---

## Confidence levels

Evidence written via the runtime ingestion paths gets the following confidence scores:

| Age of newest event | Confidence |
|---------------------|------------|
| ≤ 7 days | `high` |
| 8 – 30 days | `medium` |
| > 30 days | `low` |

Static call-site evidence (from `POST /v1/call-sites`) and collection file evidence
always start at `medium` or `low`.

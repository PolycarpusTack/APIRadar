import { useEffect, useState } from 'react'
import { Activity, AlertTriangle, CheckCircle, Clock } from 'lucide-react'
import { api, ApiError } from '../lib/apiClient'

interface CoverageRow {
  consumer_id: string
  consumer_name: string
  service_id: string
  service_name: string
  source_type: string
  event_count: number
  last_seen_at: string | null
  is_stale: boolean
}

const SOURCE_LABELS: Record<string, string> = {
  runtime_usage: 'Runtime',
  static_call_site: 'Static scan',
  collection_file: 'Collection file',
}

function SourceBadge({ type }: { type: string }) {
  const label = SOURCE_LABELS[type] ?? type
  const colours: Record<string, string> = {
    runtime_usage: 'var(--cobalt)',
    static_call_site: '#7c3aed',
    collection_file: '#0891b2',
  }
  return (
    <span
      className="inline-flex items-center gap-1 rounded-full px-2 py-0.5 text-[10px] font-semibold"
      style={{
        background: colours[type] ?? '#6b7280',
        color: '#fff',
        fontFamily: 'var(--font-mono)',
      }}
    >
      <Activity className="h-2.5 w-2.5" />
      {label}
    </span>
  )
}

function StaleIndicator({ isStale, lastSeen }: { isStale: boolean; lastSeen: string | null }) {
  if (!lastSeen) {
    return (
      <span className="text-[11px]" style={{ color: 'var(--text-3)' }}>
        —
      </span>
    )
  }
  const d = new Date(lastSeen)
  const label = d.toLocaleDateString(undefined, { month: 'short', day: 'numeric', year: 'numeric' })
  if (isStale) {
    return (
      <span className="inline-flex items-center gap-1 text-[11px]" style={{ color: '#f59e0b' }}>
        <AlertTriangle className="h-3 w-3" />
        {label}
      </span>
    )
  }
  return (
    <span className="inline-flex items-center gap-1 text-[11px]" style={{ color: 'var(--text-2)' }}>
      <CheckCircle className="h-3 w-3" style={{ color: '#22c55e' }} />
      {label}
    </span>
  )
}

export default function EvidenceCoveragePage() {
  const [rows, setRows] = useState<CoverageRow[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    api.get<CoverageRow[]>('/v1/evidence/coverage')
      .then((data) => setRows(Array.isArray(data) ? data : []))
      .catch((e) => setError(e instanceof ApiError ? e.message : String(e)))
      .finally(() => setLoading(false))
  }, [])

  const staleCount = rows.filter((r) => r.is_stale).length
  const totalConsumers = new Set(rows.map((r) => r.consumer_id)).size
  const totalServices = new Set(rows.map((r) => r.service_id)).size

  return (
    <div className="p-8 max-w-6xl mx-auto">
      <div className="mb-6">
        <h1
          className="text-2xl font-bold tracking-[-0.5px]"
          style={{ fontFamily: 'var(--font-head)', color: 'var(--text-1)' }}
        >
          Evidence Coverage
        </h1>
        <p className="mt-1 text-[13px]" style={{ color: 'var(--text-2)' }}>
          Runtime and static evidence collected per consumer × service pair. Rows older than 7 days are
          flagged as stale.
        </p>
      </div>

      {/* Summary chips */}
      <div className="mb-6 flex gap-3 flex-wrap">
        {[
          { label: 'Consumers tracked', value: totalConsumers },
          { label: 'Services covered', value: totalServices },
          { label: 'Coverage rows', value: rows.length },
          { label: 'Stale rows', value: staleCount, warn: staleCount > 0 },
        ].map(({ label, value, warn }) => (
          <div
            key={label}
            className="flex flex-col rounded-lg px-4 py-3 min-w-[120px]"
            style={{
              background: 'var(--bg-surface)',
              border: `1px solid ${warn ? '#f59e0b40' : 'var(--border)'}`,
            }}
          >
            <span
              className="text-[10px] font-semibold uppercase tracking-[1px]"
              style={{ color: warn ? '#f59e0b' : 'var(--text-dim)' }}
            >
              {label}
            </span>
            <span
              className="text-2xl font-bold"
              style={{ fontFamily: 'var(--font-head)', color: warn ? '#f59e0b' : 'var(--text-1)' }}
            >
              {value}
            </span>
          </div>
        ))}
      </div>

      {/* Stale warning callout */}
      {staleCount > 0 && (
        <div
          className="mb-5 flex items-start gap-3 rounded-lg px-4 py-3"
          style={{ background: '#78350f18', border: '1px solid #f59e0b40' }}
        >
          <AlertTriangle className="h-4 w-4 flex-shrink-0 mt-0.5" style={{ color: '#f59e0b' }} />
          <div>
            <p className="text-[12.5px] font-semibold" style={{ color: '#f59e0b' }}>
              {staleCount} stale coverage row{staleCount > 1 ? 's' : ''}
            </p>
            <p className="text-[11.5px] mt-0.5" style={{ color: 'var(--text-2)' }}>
              These consumer × service pairs have not sent evidence in the last 7 days. Check that the SDK
              is deployed and the radar-api URL is reachable.
            </p>
          </div>
        </div>
      )}

      {/* Table */}
      <div
        className="rounded-xl overflow-hidden"
        style={{ border: '1px solid var(--border)', background: 'var(--bg-surface)' }}
      >
        {loading ? (
          <div className="flex items-center justify-center h-48">
            <Clock className="h-5 w-5 animate-spin" style={{ color: 'var(--text-3)' }} />
          </div>
        ) : error ? (
          <div className="flex items-center justify-center h-48">
            <p className="text-[13px]" style={{ color: '#ef4444' }}>
              {error}
            </p>
          </div>
        ) : rows.length === 0 ? (
          <div className="flex flex-col items-center justify-center h-48 gap-2">
            <Activity className="h-8 w-8" style={{ color: 'var(--text-dim)' }} />
            <p className="text-[13px]" style={{ color: 'var(--text-3)' }}>
              No evidence collected yet. Deploy the SDK to start seeing coverage.
            </p>
          </div>
        ) : (
          <table className="w-full text-[12.5px]">
            <thead>
              <tr style={{ borderBottom: '1px solid var(--border)' }}>
                {['Consumer', 'Service', 'Source', 'Events', 'Last seen'].map((h) => (
                  <th
                    key={h}
                    className="px-4 py-3 text-left font-semibold uppercase tracking-[0.8px] text-[10px]"
                    style={{ color: 'var(--text-dim)' }}
                  >
                    {h}
                  </th>
                ))}
              </tr>
            </thead>
            <tbody>
              {rows.map((row, i) => (
                <tr
                  key={`${row.consumer_id}-${row.service_id}-${row.source_type}`}
                  style={{
                    borderBottom: i < rows.length - 1 ? '1px solid var(--border)' : undefined,
                    background: row.is_stale ? '#78350f08' : undefined,
                  }}
                >
                  <td className="px-4 py-3">
                    <span className="font-medium" style={{ color: 'var(--text-1)' }}>
                      {row.consumer_name || row.consumer_id}
                    </span>
                    <span
                      className="ml-1.5 font-mono text-[10px]"
                      style={{ color: 'var(--text-3)' }}
                    >
                      {row.consumer_id.slice(0, 8)}
                    </span>
                  </td>
                  <td className="px-4 py-3" style={{ color: 'var(--text-2)' }}>
                    {row.service_name || row.service_id}
                  </td>
                  <td className="px-4 py-3">
                    <SourceBadge type={row.source_type} />
                  </td>
                  <td
                    className="px-4 py-3 font-mono"
                    style={{ color: 'var(--text-2)' }}
                  >
                    {row.event_count.toLocaleString()}
                  </td>
                  <td className="px-4 py-3">
                    <StaleIndicator isStale={row.is_stale} lastSeen={row.last_seen_at} />
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </div>

      {/* SDK callout */}
      <div
        className="mt-6 rounded-lg px-5 py-4"
        style={{ background: 'var(--bg-raised)', border: '1px solid var(--border)' }}
      >
        <p className="text-[12px] font-semibold mb-2" style={{ color: 'var(--text-1)' }}>
          Add evidence collection to your service
        </p>
        <div className="flex flex-col gap-2">
          <div>
            <p className="text-[11px] font-semibold mb-1" style={{ color: 'var(--text-dim)' }}>
              Node.js / Express
            </p>
            <pre
              className="rounded px-3 py-2 text-[11px] overflow-x-auto"
              style={{ background: 'var(--bg-base)', color: 'var(--text-2)', fontFamily: 'var(--font-mono)' }}
            >
{`npm install @radar-monitor/sdk
const { expressMiddleware } = require('@radar-monitor/sdk')
app.use(expressMiddleware({ radarUrl, consumerId, serviceId }))`}
            </pre>
          </div>
          <div>
            <p className="text-[11px] font-semibold mb-1" style={{ color: 'var(--text-dim)' }}>
              Python / FastAPI
            </p>
            <pre
              className="rounded px-3 py-2 text-[11px] overflow-x-auto"
              style={{ background: 'var(--bg-base)', color: 'var(--text-2)', fontFamily: 'var(--font-mono)' }}
            >
{`pip install radar-monitor-sdk
from radar_monitor import RadarMiddleware
app.add_middleware(RadarMiddleware, radar_url=..., consumer_id=..., service_id=...)`}
            </pre>
          </div>
        </div>
      </div>
    </div>
  )
}

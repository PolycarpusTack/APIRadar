import { useEffect, useState } from 'react'
import { Activity, CheckCircle2, AlertCircle, Info, ChevronRight } from 'lucide-react'
import { Link } from 'react-router-dom'
import PageHeader from '../components/PageHeader'
import KpiCard from '../components/KpiCard'
import { api } from '../lib/apiClient'
import { buildDiffBuckets } from '../lib/diffTimeline'

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

interface Summary {
  breaking_changes_30d: number
  consumers_at_risk: number
  services_count: number
}

interface ReadinessItem {
  name: string
  status: 'ok' | 'missing' | 'warn'
  hint: string
  count: number
  last_at?: string | null
}

interface Readiness {
  overall: 'ready' | 'setup_required'
  items: ReadinessItem[]
}

interface DiffEntry {
  id: string
  created_at: string
  breaking_count: number
  risky_count: number
  safe_count: number
}

// ---------------------------------------------------------------------------
// ApiStatusBadge
// ---------------------------------------------------------------------------

type ApiStatus = 'checking' | 'online' | 'offline'

function ApiStatusBadge() {
  const [status, setStatus] = useState<ApiStatus>('checking')

  useEffect(() => {
    let cancelled = false
    async function check() {
      try {
        await api.get<{ status: string }>('/health', { signal: AbortSignal.timeout(4000) })
        if (!cancelled) setStatus('online')
      } catch {
        if (!cancelled) setStatus('offline')
      }
    }
    void check()
    const id = setInterval(() => void check(), 15_000)
    return () => { cancelled = true; clearInterval(id) }
  }, [])

  const dot = {
    checking: { bg: 'var(--amber)',   pulse: 'animate-pulse' },
    online:   { bg: 'var(--teal)',    pulse: '' },
    offline:  { bg: 'var(--red)',     pulse: 'animate-pulse' },
  }[status]

  const label = {
    checking: 'Connecting to radar-api…',
    online:   'radar-api  online',
    offline:  'radar-api  offline',
  }[status]

  return (
    <div
      className="inline-flex items-center gap-3 rounded-lg px-4 py-2.5"
      style={{ background: 'var(--bg-surface)', border: '1px solid var(--border)' }}
    >
      <span
        className={`h-2 w-2 flex-shrink-0 rounded-full ${dot.pulse}`}
        style={{
          background: dot.bg,
          boxShadow: status === 'online' ? 'var(--glow-teal)' : undefined,
        }}
      />
      <span
        className="text-[12.5px] font-medium"
        style={{ fontFamily: 'var(--font-mono)', color: 'var(--text-2)' }}
      >
        {label}
      </span>
      <Activity className="h-3.5 w-3.5" style={{ color: 'var(--text-3)' }} />
    </div>
  )
}

// ---------------------------------------------------------------------------
// ReadinessPanel — shows setup progress when overall !== 'ready'
// ---------------------------------------------------------------------------

const ITEM_META: Record<string, { label: string; link?: string }> = {
  db_connected:               { label: 'Database connected' },
  service_registered:         { label: 'Service registered',         link: '/services' },
  diff_recorded:              { label: 'First diff recorded',        link: '/diffs' },
  consumer_registered:        { label: 'Consumer registered',        link: '/consumers' },
  catalog_source_configured:  { label: 'Catalog source configured',  link: '/catalog-sources' },
  webhook_configured:         { label: 'Webhook configured',         link: '/settings' },
}

function ReadinessPanel({ items }: { items: ReadinessItem[] }) {
  const critical = items.filter(i => i.name !== 'catalog_source_configured' && i.name !== 'webhook_configured')
  const optional = items.filter(i => i.name === 'catalog_source_configured' || i.name === 'webhook_configured')

  const doneCount = critical.filter(i => i.status === 'ok').length
  const pct = Math.round((doneCount / critical.length) * 100)

  function StatusIcon({ status }: { status: ReadinessItem['status'] }) {
    if (status === 'ok')      return <CheckCircle2 className="h-4 w-4 flex-shrink-0" style={{ color: 'var(--teal)' }} />
    if (status === 'missing') return <AlertCircle   className="h-4 w-4 flex-shrink-0" style={{ color: 'var(--red)' }} />
    return                           <Info           className="h-4 w-4 flex-shrink-0" style={{ color: 'var(--amber)' }} />
  }

  function Row({ item }: { item: ReadinessItem }) {
    const meta = ITEM_META[item.name] ?? { label: item.name }
    const content = (
      <div
        className="flex items-start gap-3 rounded-md px-3 py-2.5 transition-colors"
        style={{
          background: item.status === 'missing' ? 'color-mix(in srgb, var(--red) 5%, transparent)' : 'transparent',
          border: '1px solid',
          borderColor: item.status === 'missing' ? 'color-mix(in srgb, var(--red) 20%, transparent)' : 'transparent',
        }}
      >
        <StatusIcon status={item.status} />
        <div className="flex-1 min-w-0">
          <div className="flex items-center gap-2">
            <span
              className="text-[12.5px] font-medium"
              style={{ color: item.status === 'ok' ? 'var(--text-2)' : 'var(--text-1)' }}
            >
              {meta.label}
            </span>
            {item.count > 0 && item.name !== 'db_connected' && (
              <span
                className="text-[10.5px] rounded-full px-1.5 py-0.5"
                style={{ background: 'var(--bg-raised)', color: 'var(--text-dim)', fontFamily: 'var(--font-mono)' }}
              >
                {item.count}
              </span>
            )}
            {meta.link && item.status !== 'ok' && (
              <ChevronRight className="h-3 w-3 ml-auto flex-shrink-0" style={{ color: 'var(--text-dim)' }} />
            )}
          </div>
          {item.hint && item.status !== 'ok' && (
            <p className="mt-0.5 text-[11px] leading-relaxed" style={{ color: 'var(--text-3)', fontFamily: 'var(--font-mono)' }}>
              {item.hint}
            </p>
          )}
        </div>
      </div>
    )

    if (meta.link && item.status !== 'ok') {
      return <Link to={meta.link} className="block hover:no-underline">{content}</Link>
    }
    return content
  }

  return (
    <div
      className="rounded-xl p-5"
      style={{ background: 'var(--bg-surface)', border: '1px solid var(--border)' }}
    >
      {/* Header + progress bar */}
      <div className="mb-4">
        <div className="flex items-center justify-between mb-2">
          <p className="text-[9.5px] font-semibold uppercase tracking-[1.2px]" style={{ color: 'var(--text-dim)' }}>
            Setup progress
          </p>
          <span className="text-[11px]" style={{ fontFamily: 'var(--font-mono)', color: 'var(--text-3)' }}>
            {doneCount}/{critical.length} complete
          </span>
        </div>
        <div className="h-1.5 rounded-full overflow-hidden" style={{ background: 'var(--bg-raised)' }}>
          <div
            className="h-full rounded-full transition-all duration-500"
            style={{ width: `${pct}%`, background: pct === 100 ? 'var(--teal)' : 'var(--cobalt)' }}
          />
        </div>
      </div>

      {/* Critical items */}
      <div className="space-y-1 mb-4">
        {critical.map(item => <Row key={item.name} item={item} />)}
      </div>

      {/* Optional items */}
      <div>
        <p className="text-[9px] font-semibold uppercase tracking-[1.2px] mb-1 px-1" style={{ color: 'var(--text-dim)' }}>
          Optional
        </p>
        <div className="space-y-1">
          {optional.map(item => <Row key={item.name} item={item} />)}
        </div>
      </div>
    </div>
  )
}

// ---------------------------------------------------------------------------
// DiffTimeline bar chart
// ---------------------------------------------------------------------------

function DiffTimeline({ diffs }: { diffs: DiffEntry[] }) {
  const DAYS = 30
  // Bucket by *local* calendar day so a diff lands on the same day the diffs
  // table (which formats in local time) shows it under.
  const dayBuckets = buildDiffBuckets(diffs, new Date(), DAYS)
  const buckets: Record<string, { breaking: number; risky: number; safe: number }> =
    Object.fromEntries(dayBuckets.map(b => [b.key, { breaking: b.breaking, risky: b.risky, safe: b.safe }]))

  const keys = dayBuckets.map(b => b.key)
  const maxTotal = Math.max(1, ...keys.map(k => buckets[k].breaking + buckets[k].risky + buckets[k].safe))
  const W = 600; const H = 72
  const barW = (W / DAYS) * 0.6
  const gap = (W / DAYS) * 0.4

  return (
    <div>
      <p className="mb-2 text-[9.5px] font-semibold uppercase tracking-[1.2px]" style={{ color: 'var(--text-dim)' }}>
        Diff activity — last 30 days
      </p>
      <div
        className="rounded-lg px-4 pt-4 pb-3 overflow-hidden"
        style={{ background: 'var(--bg-surface)', border: '1px solid var(--border)' }}
      >
        <svg viewBox={`0 0 ${W} ${H + 18}`} className="w-full" style={{ height: 90 }}>
          {keys.map((day, i) => {
            const { breaking, risky, safe } = buckets[day]
            const total = breaking + risky + safe
            const x = i * (W / DAYS) + gap / 2
            const totalH = (total / maxTotal) * H
            const breakH = (breaking / Math.max(1, total)) * totalH
            const riskyH = (risky / Math.max(1, total)) * totalH
            const safeH = totalH - breakH - riskyH
            const isMonday = new Date(day).getDay() === 1
            const labelDay = i % 5 === 0 || i === 0
            return (
              <g key={day}>
                {safe > 0    && <rect x={x} y={H - totalH}              width={barW} height={safeH}  fill="var(--teal)"  opacity="0.7"  rx={1} />}
                {risky > 0   && <rect x={x} y={H - totalH + safeH}      width={barW} height={riskyH} fill="var(--amber)" opacity="0.85" rx={1} />}
                {breaking > 0 && <rect x={x} y={H - totalH + safeH + riskyH} width={barW} height={breakH} fill="var(--red)" opacity="0.9" rx={1} />}
                {isMonday && <line x1={x - gap / 2} y1={0} x2={x - gap / 2} y2={H} stroke="var(--border)" strokeWidth="0.5" />}
                {labelDay && (
                  <text x={x + barW / 2} y={H + 14} textAnchor="middle" fontSize="8" fill="var(--text-dim)" fontFamily="var(--font-mono)">
                    {day.slice(5)}
                  </text>
                )}
              </g>
            )
          })}
        </svg>
        <div className="flex items-center gap-4 mt-1" style={{ fontSize: '10px', fontFamily: 'var(--font-mono)', color: 'var(--text-dim)' }}>
          {[['var(--red)', 'breaking'], ['var(--amber)', 'risky'], ['var(--teal)', 'safe']].map(([color, label]) => (
            <span key={label} style={{ display: 'inline-flex', alignItems: 'center', gap: 4 }}>
              <span style={{ width: 8, height: 8, borderRadius: 2, background: color, opacity: 0.9, display: 'inline-block' }} />
              {label}
            </span>
          ))}
        </div>
      </div>
    </div>
  )
}

// ---------------------------------------------------------------------------
// Quick start code block
// ---------------------------------------------------------------------------

function QuickStart() {
  const lines = [
    { type: 'comment', text: '# Compare two spec files and post to radar-api' },
    { type: 'cmd',     text: 'radar check \\' },
    { type: 'arg',     text: '  --base old.yaml --head new.yaml \\' },
    { type: 'arg',     text: '  --api-url http://localhost:8080 \\' },
    { type: 'arg',     text: '  --service-id <uuid>' },
    { type: 'blank',   text: '' },
    { type: 'comment', text: '# Register a consumer and subscribe it to a service' },
    { type: 'cmd',     text: 'radar register \\' },
    { type: 'arg',     text: '  --api-url http://localhost:8080 \\' },
    { type: 'arg',     text: '  --service-id <uuid> \\' },
    { type: 'arg',     text: '  --consumer-name checkout-svc \\' },
    { type: 'arg',     text: '  --repo-url https://github.com/org/checkout \\' },
    { type: 'arg',     text: '  --owner-team payments --contact ops@example.com' },
  ]
  return (
    <section>
      <p className="mb-1 text-[9.5px] font-semibold uppercase tracking-[1.2px]" style={{ color: 'var(--text-dim)' }}>
        Quick Start
      </p>
      <p className="mb-4 text-[13px] leading-relaxed" style={{ color: 'var(--text-3)' }}>
        Add the CLI to your CI pipeline. Run{' '}
        <code
          className="rounded px-1.5 py-0.5 text-[11.5px]"
          style={{ background: 'var(--bg-raised)', border: '1px solid var(--border)', color: 'var(--teal)', fontFamily: 'var(--font-mono)' }}
        >
          radar check
        </code>{' '}
        on any pull request to automatically detect schema drift and notify affected consumers.
      </p>
      <div
        className="overflow-x-auto rounded-lg p-4"
        style={{ background: 'var(--bg-raised)', border: '1px solid var(--border)', fontFamily: 'var(--font-mono)', fontSize: '12px', lineHeight: '1.7' }}
      >
        {lines.map((line, i) =>
          line.type === 'blank' ? (
            <div key={i} className="h-3" />
          ) : (
            <div key={i} style={{ color: line.type === 'comment' ? 'var(--text-dim)' : line.type === 'arg' ? 'var(--text-2)' : 'var(--neon-green)' }}>
              {line.text}
            </div>
          )
        )}
      </div>
    </section>
  )
}

// ---------------------------------------------------------------------------
// Page
// ---------------------------------------------------------------------------

export default function HomePage() {
  const [summary, setSummary] = useState<Summary | null>(null)
  const [readiness, setReadiness] = useState<Readiness | null>(null)
  const [diffs, setDiffs] = useState<DiffEntry[]>([])

  useEffect(() => {
    api.get<Summary>('/v1/summary').then(setSummary).catch(() => {})
    api.get<Readiness>('/v1/readiness').then(setReadiness).catch(() => {})
    api.get<DiffEntry[]>('/v1/diffs?limit=200').then(setDiffs).catch(() => {})
  }, [])

  const fmt = (n: number | undefined) => n === undefined ? '—' : String(n)
  const setupRequired = readiness?.overall === 'setup_required'

  return (
    <div>
      <PageHeader
        tag="Dashboard"
        title="API Contract"
        titleAccent="Radar Monitor"
        description="Detect breaking changes before they reach your consumers. Connect your CI pipeline to get blast-radius alerts on every pull request."
      />

      <div className="px-14 py-10 space-y-10">
        <ApiStatusBadge />

        {/* Setup checklist — visible until all critical items are green */}
        {readiness && setupRequired && (
          <ReadinessPanel items={readiness.items} />
        )}

        {/* KPI row */}
        <section>
          <p className="mb-3 text-[9.5px] font-semibold uppercase tracking-[1.2px]" style={{ color: 'var(--text-dim)' }}>
            Last 30 days
          </p>
          <div className="grid grid-cols-1 gap-4 sm:grid-cols-3">
            <KpiCard
              label="Breaking Changes"
              value={fmt(summary?.breaking_changes_30d)}
              meta="across all services"
              variant="red"
            />
            <KpiCard
              label="Consumers at Risk"
              value={fmt(summary?.consumers_at_risk)}
              meta="with active subscriptions"
              variant="amber"
            />
            <KpiCard
              label="APIs Monitored"
              value={fmt(summary?.services_count)}
              meta="registered services"
              variant="teal"
            />
          </div>
        </section>

        {/* Timeline */}
        <DiffTimeline diffs={diffs} />

        {/* Quick start — always shown until setup is complete */}
        {setupRequired && <QuickStart />}
      </div>
    </div>
  )
}

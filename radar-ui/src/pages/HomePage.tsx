import { useEffect, useState } from 'react'
import { Activity } from 'lucide-react'
import PageHeader from '../components/PageHeader'
import KpiCard from '../components/KpiCard'
import FirstRunBanner from '../components/FirstRunBanner'
import { api } from '../lib/apiClient'

type ApiStatus = 'checking' | 'online' | 'offline'

interface Summary {
  breaking_changes_30d: number
  consumers_at_risk: number
  services_count: number
}

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

function CodeExample() {
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
    <div
      className="overflow-x-auto rounded-lg p-4"
      style={{
        background: 'var(--bg-raised)',
        border: '1px solid var(--border)',
        fontFamily: 'var(--font-mono)',
        fontSize: '12px',
        lineHeight: '1.7',
      }}
    >
      {lines.map((line, i) =>
        line.type === 'blank' ? (
          <div key={i} className="h-3" />
        ) : (
          <div
            key={i}
            style={{
              color:
                line.type === 'comment'
                  ? 'var(--text-dim)'
                  : line.type === 'arg'
                  ? 'var(--text-2)'
                  : 'var(--neon-green)',
            }}
          >
            {line.text}
          </div>
        )
      )}
    </div>
  )
}

interface DiffEntry {
  created_at: string
  breaking_count: number
  risky_count: number
  safe_count: number
}

function DiffTimeline({ diffs }: { diffs: DiffEntry[] }) {
  const DAYS = 30
  const today = new Date()
  today.setHours(23, 59, 59, 999)

  // Build a bucket per day label → { breaking, risky, safe }
  const buckets: Record<string, { breaking: number; risky: number; safe: number }> = {}
  for (let i = DAYS - 1; i >= 0; i--) {
    const d = new Date(today)
    d.setDate(d.getDate() - i)
    const key = d.toISOString().slice(0, 10)
    buckets[key] = { breaking: 0, risky: 0, safe: 0 }
  }

  for (const diff of diffs) {
    const day = diff.created_at.slice(0, 10)
    if (buckets[day]) {
      buckets[day].breaking += diff.breaking_count
      buckets[day].risky += diff.risky_count
      buckets[day].safe += diff.safe_count
    }
  }

  const keys = Object.keys(buckets)
  const maxTotal = Math.max(1, ...keys.map(k => buckets[k].breaking + buckets[k].risky + buckets[k].safe))

  const W = 600
  const H = 72
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
            const isFirst = i === 0
            const labelDay = i % 5 === 0 || isFirst

            return (
              <g key={day}>
                {/* safe (bottom) */}
                {safe > 0 && (
                  <rect x={x} y={H - totalH} width={barW} height={safeH}
                    fill="var(--teal)" opacity="0.7" rx={1} />
                )}
                {/* risky (middle) */}
                {risky > 0 && (
                  <rect x={x} y={H - totalH + safeH} width={barW} height={riskyH}
                    fill="var(--amber)" opacity="0.85" rx={1} />
                )}
                {/* breaking (top) */}
                {breaking > 0 && (
                  <rect x={x} y={H - totalH + safeH + riskyH} width={barW} height={breakH}
                    fill="var(--red)" opacity="0.9" rx={1} />
                )}
                {/* monday line */}
                {isMonday && (
                  <line x1={x - gap / 2} y1={0} x2={x - gap / 2} y2={H}
                    stroke="var(--border)" strokeWidth="0.5" />
                )}
                {/* day label every 5 */}
                {labelDay && (
                  <text x={x + barW / 2} y={H + 14}
                    textAnchor="middle" fontSize="8"
                    fill="var(--text-dim)"
                    fontFamily="var(--font-mono)"
                  >
                    {day.slice(5)}
                  </text>
                )}
              </g>
            )
          })}
        </svg>
        <div className="flex items-center gap-4 mt-1" style={{ fontSize: '10px', fontFamily: 'var(--font-mono)', color: 'var(--text-dim)' }}>
          <span style={{ display: 'inline-flex', alignItems: 'center', gap: 4 }}>
            <span style={{ width: 8, height: 8, borderRadius: 2, background: 'var(--red)', opacity: 0.9, display: 'inline-block' }} /> breaking
          </span>
          <span style={{ display: 'inline-flex', alignItems: 'center', gap: 4 }}>
            <span style={{ width: 8, height: 8, borderRadius: 2, background: 'var(--amber)', opacity: 0.85, display: 'inline-block' }} /> risky
          </span>
          <span style={{ display: 'inline-flex', alignItems: 'center', gap: 4 }}>
            <span style={{ width: 8, height: 8, borderRadius: 2, background: 'var(--teal)', opacity: 0.7, display: 'inline-block' }} /> safe
          </span>
        </div>
      </div>
    </div>
  )
}

export default function HomePage() {
  const [summary, setSummary] = useState<Summary | null>(null)
  const [diffs, setDiffs] = useState<DiffEntry[]>([])

  useEffect(() => {
    api.get<Summary>('/v1/summary')
      .then(setSummary)
      .catch(() => { /* server may not be reachable yet — KPIs stay as — */ })

    api.get<DiffEntry[]>('/v1/diffs?limit=200')
      .then(setDiffs)
      .catch(() => { /* non-fatal */ })
  }, [])

  const fmt = (n: number | undefined) => n === undefined ? '—' : String(n)

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

        {/* First-run wizard — visible only when no services have been registered yet */}
        {summary !== null && summary.services_count === 0 && <FirstRunBanner />}

        {/* KPI row */}
        <section>
          <p
            className="mb-3 text-[9.5px] font-semibold uppercase tracking-[1.2px]"
            style={{ color: 'var(--text-dim)' }}
          >
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

        {/* Quick start */}
        <section>
          <p
            className="mb-1 text-[9.5px] font-semibold uppercase tracking-[1.2px]"
            style={{ color: 'var(--text-dim)' }}
          >
            Quick Start
          </p>
          <p className="mb-4 text-[13px] leading-relaxed" style={{ color: 'var(--text-3)' }}>
            Add the CLI to your CI pipeline. Run{' '}
            <code
              className="rounded px-1.5 py-0.5 text-[11.5px]"
              style={{
                background: 'var(--bg-raised)',
                border: '1px solid var(--border)',
                color: 'var(--teal)',
                fontFamily: 'var(--font-mono)',
              }}
            >
              radar check
            </code>{' '}
            on any pull request to automatically detect schema drift and notify affected consumers.
          </p>
          <CodeExample />
        </section>
      </div>
    </div>
  )
}

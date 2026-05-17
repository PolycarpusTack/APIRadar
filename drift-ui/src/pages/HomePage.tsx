import { useEffect, useState } from 'react'
import { Activity } from 'lucide-react'
import PageHeader from '../components/PageHeader'
import KpiCard from '../components/KpiCard'

type ApiStatus = 'checking' | 'online' | 'offline'

function ApiStatusBadge() {
  const [status, setStatus] = useState<ApiStatus>('checking')

  useEffect(() => {
    let cancelled = false

    async function check() {
      try {
        const res = await fetch('/health', { signal: AbortSignal.timeout(4000) })
        if (!cancelled) setStatus(res.ok ? 'online' : 'offline')
      } catch {
        if (!cancelled) setStatus('offline')
      }
    }

    void check()
    const id = setInterval(() => void check(), 15_000)
    return () => { cancelled = true; clearInterval(id) }
  }, [])

  const dot = {
    checking: { bg: 'var(--amber)',   animate: 'animate-pulse' },
    online:   { bg: 'var(--teal)',    animate: '' },
    offline:  { bg: 'var(--red)',     animate: 'animate-pulse' },
  }[status]

  const label = {
    checking: 'Connecting to drift-api…',
    online:   'drift-api  online',
    offline:  'drift-api  offline',
  }[status]

  return (
    <div
      className="inline-flex items-center gap-3 rounded-lg px-4 py-2.5"
      style={{
        background: 'var(--bg-surface)',
        border: '1px solid var(--border)',
      }}
    >
      <span
        className={`h-2 w-2 flex-shrink-0 rounded-full ${dot.animate}`}
        style={{ background: dot.bg, boxShadow: status === 'online' ? 'var(--glow-teal)' : undefined }}
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
    { type: 'comment', text: '# Compare two git refs and post to drift-api' },
    { type: 'cmd',     text: 'drift check \\' },
    { type: 'arg',     text: '  --base main --head HEAD \\' },
    { type: 'arg',     text: '  --api-url http://localhost:8080 \\' },
    { type: 'arg',     text: '  --service-id my-service' },
    { type: 'blank',   text: '' },
    { type: 'comment', text: '# Register a consumer' },
    { type: 'cmd',     text: 'drift register \\' },
    { type: 'arg',     text: '  --api-url http://localhost:8080 \\' },
    { type: 'arg',     text: '  --service-id my-service \\' },
    { type: 'arg',     text: '  --consumer-name checkout-service' },
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

export default function HomePage() {
  return (
    <div>
      <PageHeader
        tag="Dashboard"
        title="API Contract"
        titleAccent="Drift Monitor"
        description="Detect breaking changes before they reach your consumers. Connect your CI pipeline to get blast-radius alerts on every pull request."
      />

      <div className="px-14 py-10 space-y-10">
        {/* API status */}
        <div>
          <ApiStatusBadge />
        </div>

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
              value="—"
              meta="across all services"
              variant="red"
            />
            <KpiCard
              label="Consumers at Risk"
              value="—"
              meta="with active subscriptions"
              variant="amber"
            />
            <KpiCard
              label="APIs Monitored"
              value="—"
              meta="registered services"
              variant="teal"
            />
          </div>
        </section>

        {/* Quick start */}
        <section>
          <p
            className="mb-1 text-[9.5px] font-semibold uppercase tracking-[1.2px]"
            style={{ color: 'var(--text-dim)' }}
          >
            Quick Start
          </p>
          <p
            className="mb-4 text-[13px] leading-relaxed"
            style={{ color: 'var(--text-3)' }}
          >
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
              drift check
            </code>{' '}
            on any pull request to automatically detect schema drift and notify affected consumers.
          </p>
          <CodeExample />
        </section>
      </div>
    </div>
  )
}

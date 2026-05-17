import { useEffect, useState } from 'react'

type ApiStatus = 'checking' | 'online' | 'offline'

interface KpiCardProps {
  label: string
  value: string
  color?: string
}

function KpiCard({ label, value, color = 'text-white' }: KpiCardProps) {
  return (
    <div className="rounded-xl border border-white/10 bg-white/5 px-6 py-5">
      <p className="text-xs font-medium text-[var(--text-dim)] uppercase tracking-wider mb-2">
        {label}
      </p>
      <p className={`text-3xl font-semibold tabular-nums ${color}`}>{value}</p>
    </div>
  )
}

function StatusDot({ status }: { status: ApiStatus }) {
  if (status === 'checking') {
    return (
      <span className="inline-block w-2.5 h-2.5 rounded-full bg-[var(--amber)] animate-pulse" />
    )
  }
  if (status === 'online') {
    return <span className="inline-block w-2.5 h-2.5 rounded-full bg-[var(--teal)]" />
  }
  return <span className="inline-block w-2.5 h-2.5 rounded-full bg-[var(--red)]" />
}

function ApiStatusCard() {
  const [status, setStatus] = useState<ApiStatus>('checking')

  useEffect(() => {
    let cancelled = false

    async function checkHealth() {
      try {
        const res = await fetch('/health', { signal: AbortSignal.timeout(4000) })
        if (!cancelled) {
          setStatus(res.ok ? 'online' : 'offline')
        }
      } catch {
        if (!cancelled) {
          setStatus('offline')
        }
      }
    }

    void checkHealth()

    const interval = setInterval(() => {
      void checkHealth()
    }, 15_000)

    return () => {
      cancelled = true
      clearInterval(interval)
    }
  }, [])

  const label =
    status === 'checking' ? 'Connecting…' : status === 'online' ? 'drift-api online' : 'drift-api offline'

  return (
    <div className="flex items-center gap-3 rounded-xl border border-white/10 bg-white/5 px-6 py-4 w-fit">
      <StatusDot status={status} />
      <span className="text-sm font-medium text-[var(--text-dim)]">{label}</span>
    </div>
  )
}

export default function HomePage() {
  return (
    <div className="px-8 py-10 space-y-8">
      <div className="space-y-2">
        <h1 className="text-2xl font-bold tracking-tight text-white">
          API Contract Drift Monitor
        </h1>
        <p className="text-sm text-[var(--text-dim)]">
          Detect breaking changes before they reach your consumers.
        </p>
      </div>

      <ApiStatusCard />

      <section className="space-y-3">
        <h2 className="text-xs font-semibold uppercase tracking-widest text-[var(--text-dim)]">
          Last 30 days
        </h2>
        <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-4">
          <KpiCard
            label="Breaking Changes"
            value="-"
            color="text-[var(--red)]"
          />
          <KpiCard
            label="Consumers at Risk"
            value="-"
            color="text-[var(--amber)]"
          />
          <KpiCard
            label="APIs Monitored"
            value="-"
            color="text-[var(--teal)]"
          />
        </div>
      </section>
    </div>
  )
}

import { useEffect, useState } from 'react'
import { useParams, useNavigate } from 'react-router-dom'
import { ArrowLeft } from 'lucide-react'
import Badge from '../components/Badge'
import { api, ApiError } from '../lib/apiClient'

interface Consumer {
  id: string
  name: string
  repo_url: string
  owner_team: string
  contact: string
  subscription_count: number
  last_seen: string | null
}

interface Subscription {
  id: string
  service_id: string
  service_name?: string
  opted_in_at: string
}

function formatDate(iso: string) {
  try {
    return new Date(iso).toLocaleDateString('en-GB', { day: '2-digit', month: 'short', year: 'numeric' })
  } catch {
    return iso
  }
}

export default function ConsumerDetailPage() {
  const { id } = useParams<{ id: string }>()
  const navigate = useNavigate()

  const [consumer, setConsumer] = useState<Consumer | null>(null)
  const [subs, setSubs] = useState<Subscription[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    if (!id) return

    // Load the consumer from the list endpoint (no single-consumer GET yet).
    api.get<Consumer[]>('/v1/consumers')
      .then(all => {
        const found = all.find(c => c.id === id) ?? null
        if (!found) throw new Error('Consumer not found')
        setConsumer(found)
        // Load subscriptions from the service-scoped endpoint indirectly
        // by fetching services and their consumers.
        return api.get<{ id: string; name: string }[]>('/v1/services')
      })
      .then(services =>
        Promise.all(
          services.map(svc =>
            api.get<{ id: string; name: string }[]>(`/v1/services/${svc.id}/consumers`)
              .catch(() => [] as { id: string; name: string }[])
              .then((consumers: { id: string; name: string }[]) => {
                const found = consumers.find(c => c.id === id)
                if (!found) return null
                return { id: `${svc.id}:${id}`, service_id: svc.id, service_name: svc.name, opted_in_at: '' } as Subscription
              })
          )
        )
      )
      .then(results => setSubs(results.filter(Boolean) as Subscription[]))
      .catch((e: unknown) => setError(e instanceof ApiError ? e.message : String(e)))
      .finally(() => setLoading(false))
  }, [id])

  if (loading) {
    return (
      <div className="flex items-center justify-center h-64">
        <p className="text-[12.5px]" style={{ color: 'var(--text-3)' }}>Loading…</p>
      </div>
    )
  }

  if (error || !consumer) {
    return (
      <div className="px-14 py-10">
        <p className="text-[12.5px]" style={{ color: 'var(--red)' }}>{error ?? 'Consumer not found'}</p>
      </div>
    )
  }

  return (
    <div>
      {/* Back bar */}
      <div
        className="flex items-center gap-3 border-b px-14 py-4"
        style={{ background: 'var(--bg-surface)', borderColor: 'var(--border)' }}
      >
        <button
          onClick={() => navigate('/consumers')}
          className="flex items-center gap-1.5 text-[12px] transition-colors hover:text-[var(--text-1)]"
          style={{ color: 'var(--text-3)', fontFamily: 'var(--font-mono)' }}
        >
          <ArrowLeft className="h-3.5 w-3.5" />
          All Consumers
        </button>
        <span style={{ color: 'var(--border-hi)' }}>/</span>
        <span className="text-[12px]" style={{ color: 'var(--text-2)', fontFamily: 'var(--font-mono)' }}>
          {consumer.name}
        </span>
      </div>

      {/* Header card */}
      <div
        className="border-b px-14 py-8"
        style={{ background: 'var(--bg-surface)', borderColor: 'var(--border)' }}
      >
        <p className="mb-2 text-[10.5px] font-medium uppercase tracking-[1.5px]" style={{ fontFamily: 'var(--font-mono)', color: 'var(--cobalt-mid)' }}>
          Consumer
        </p>
        <h1 className="mb-3 text-[32px] font-bold tracking-[-1px]" style={{ fontFamily: 'var(--font-head)', color: 'var(--text-1)' }}>
          {consumer.name}
        </h1>
        <div className="flex flex-wrap gap-4 text-[12.5px]" style={{ color: 'var(--text-2)' }}>
          {consumer.owner_team && (
            <span><span style={{ color: 'var(--text-3)' }}>Team</span> {consumer.owner_team}</span>
          )}
          {consumer.contact && (
            <span><span style={{ color: 'var(--text-3)' }}>Contact</span> <span style={{ fontFamily: 'var(--font-mono)' }}>{consumer.contact}</span></span>
          )}
          {consumer.last_seen && (
            <span><span style={{ color: 'var(--text-3)' }}>Last seen</span> {formatDate(consumer.last_seen)}</span>
          )}
          <span>
            <Badge variant={consumer.subscription_count > 0 ? 'cobalt' : 'neutral'}>
              {consumer.subscription_count} subscription{consumer.subscription_count !== 1 ? 's' : ''}
            </Badge>
          </span>
        </div>
        {consumer.repo_url && (
          <a
            href={consumer.repo_url}
            target="_blank"
            rel="noreferrer"
            className="mt-2 inline-block text-[11.5px] underline decoration-dotted hover:no-underline"
            style={{ color: 'var(--cobalt-mid)', fontFamily: 'var(--font-mono)' }}
          >
            {consumer.repo_url.replace(/^https?:\/\//, '')}
          </a>
        )}
      </div>

      {/* Content */}
      <div className="px-14 py-8 space-y-8">
        {/* Subscribed services */}
        <section>
          <p className="mb-3 text-[9.5px] font-semibold uppercase tracking-[1.2px]" style={{ color: 'var(--text-dim)' }}>
            Subscribed Services ({subs.length})
          </p>
          <div className="overflow-hidden rounded-lg" style={{ border: '1px solid var(--border)', background: 'var(--bg-surface)' }}>
            {subs.length === 0 ? (
              <p className="px-4 py-6 text-center text-[12.5px]" style={{ color: 'var(--text-3)' }}>
                Not subscribed to any services yet.
              </p>
            ) : (
              <table className="w-full border-collapse">
                <thead>
                  <tr>
                    {['Service', 'Service ID'].map(col => (
                      <th
                        key={col}
                        className="border-b px-3 py-2 text-left text-[10.5px] font-semibold uppercase tracking-[0.8px]"
                        style={{ background: 'var(--bg-raised)', borderColor: 'var(--border)', color: 'var(--text-3)' }}
                      >
                        {col}
                      </th>
                    ))}
                  </tr>
                </thead>
                <tbody>
                  {subs.map(sub => (
                    <tr key={sub.id} className="group" style={{ borderBottom: '1px solid var(--border)' }}>
                      <td className="px-3 py-2.5 font-medium group-hover:bg-[var(--bg-hover)]" style={{ fontSize: '12.5px', color: 'var(--text-1)' }}>
                        {sub.service_name ?? sub.service_id}
                      </td>
                      <td className="px-3 py-2.5 group-hover:bg-[var(--bg-hover)]" style={{ fontFamily: 'var(--font-mono)', fontSize: '11px', color: 'var(--text-dim)' }}>
                        {sub.service_id.slice(0, 8)}…
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            )}
          </div>
        </section>

        {/* Consumer ID for CLI reference */}
        <section>
          <p className="mb-2 text-[9.5px] font-semibold uppercase tracking-[1.2px]" style={{ color: 'var(--text-dim)' }}>CLI Reference</p>
          <div
            className="rounded-lg p-4"
            style={{ background: 'var(--bg-raised)', border: '1px solid var(--border)' }}
          >
            <p className="mb-1.5 text-[10.5px]" style={{ color: 'var(--text-3)' }}>Consumer ID (use with <code style={{ fontFamily: 'var(--font-mono)' }}>--consumer-id</code>)</p>
            <code className="text-[12px]" style={{ fontFamily: 'var(--font-mono)', color: 'var(--teal)' }}>
              {consumer.id}
            </code>
          </div>
        </section>
      </div>
    </div>
  )
}

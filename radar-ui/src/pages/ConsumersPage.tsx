import { useEffect, useState } from 'react'
import { useNavigate } from 'react-router-dom'
import { Users, Plus } from 'lucide-react'
import PageHeader from '../components/PageHeader'
import Badge from '../components/Badge'
import EmptyState from '../components/EmptyState'
import RegisterConsumerForm from '../components/RegisterConsumerForm'
import { api, ApiError } from '../lib/apiClient'

interface ConsumerRow {
  id: string
  name: string
  repo_url: string
  owner_team: string
  contact: string
  subscription_count: number
  last_seen: string | null
}

const TABLE_COLS = ['Consumer', 'Team', 'Contact', 'Subscriptions', 'Last Seen']

function formatDate(iso: string) {
  try {
    return new Date(iso).toLocaleDateString('en-GB', { day: '2-digit', month: 'short', year: 'numeric' })
  } catch {
    return iso
  }
}

function ConsumerTable({
  rows,
  onSelect,
  onRegister,
}: {
  rows: ConsumerRow[]
  onSelect: (id: string) => void
  onRegister: () => void
}) {
  if (rows.length === 0) {
    return (
      <EmptyState
        icon={Users}
        title="No consumers registered yet"
        description="Register your first consumer to start receiving blast-radius alerts when APIs it uses change."
        action={
          <button
            onClick={onRegister}
            className="flex items-center gap-1.5 rounded-md px-4 py-2 text-[12.5px] font-medium transition-opacity hover:opacity-90"
            style={{ background: 'var(--cobalt-mid)', color: '#fff' }}
          >
            <Plus className="h-3.5 w-3.5" />
            Register Consumer
          </button>
        }
      />
    )
  }

  return (
    <div className="overflow-x-auto">
      <table className="w-full border-collapse">
        <thead>
          <tr>
            {TABLE_COLS.map((col) => (
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
          {rows.map((row) => (
            <tr
              key={row.id}
              className="group cursor-pointer transition-colors"
              style={{ borderBottom: '1px solid var(--border)' }}
              onClick={() => onSelect(row.id)}
            >
              <td className="px-3 py-2.5 font-medium group-hover:bg-[var(--bg-hover)]" style={{ fontSize: '12.5px', color: 'var(--text-1)' }}>
                <div>{row.name}</div>
                {row.repo_url && (
                  <a
                    href={row.repo_url}
                    target="_blank"
                    rel="noreferrer"
                    className="text-[11px] underline decoration-dotted hover:no-underline"
                    style={{ color: 'var(--cobalt-mid)', fontFamily: 'var(--font-mono)' }}
                    onClick={(e) => e.stopPropagation()}
                  >
                    {row.repo_url.replace(/^https?:\/\//, '').slice(0, 40)}
                  </a>
                )}
              </td>
              <td className="px-3 py-2.5 group-hover:bg-[var(--bg-hover)]" style={{ fontSize: '12.5px', color: 'var(--text-2)' }}>
                {row.owner_team || <span style={{ color: 'var(--text-dim)' }}>—</span>}
              </td>
              <td className="px-3 py-2.5 group-hover:bg-[var(--bg-hover)]" style={{ fontFamily: 'var(--font-mono)', fontSize: '11.5px', color: 'var(--text-2)' }}>
                {row.contact || <span style={{ color: 'var(--text-dim)' }}>—</span>}
              </td>
              <td className="px-3 py-2.5 group-hover:bg-[var(--bg-hover)]">
                <Badge variant={row.subscription_count > 0 ? 'cobalt' : 'neutral'}>{row.subscription_count}</Badge>
              </td>
              <td className="px-3 py-2.5 group-hover:bg-[var(--bg-hover)]" style={{ fontFamily: 'var(--font-mono)', fontSize: '11.5px', color: 'var(--text-3)' }}>
                {row.last_seen ? formatDate(row.last_seen) : <span style={{ color: 'var(--text-dim)' }}>never</span>}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  )
}

export default function ConsumersPage() {
  const navigate = useNavigate()
  const [rows, setRows] = useState<ConsumerRow[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [showForm, setShowForm] = useState(false)

  function loadConsumers() {
    setLoading(true)
    api.get<ConsumerRow[]>('/v1/consumers')
      .then(setRows)
      .catch((e: unknown) => setError(e instanceof ApiError ? e.message : String(e)))
      .finally(() => setLoading(false))
  }

  useEffect(loadConsumers, [])

  function handleCreated() {
    setShowForm(false)
    loadConsumers()
  }

  return (
    <div>
      <PageHeader
        tag="Registry"
        title="Consumers"
        description="Services and teams that consume one or more of your APIs. Register a consumer to start receiving blast-radius alerts when APIs change."
      />

      <div className="px-14 py-8 flex flex-col gap-6">
        {showForm && (
          <RegisterConsumerForm
            onCreated={handleCreated}
            onClose={() => setShowForm(false)}
          />
        )}

        <div
          className="overflow-hidden rounded-lg"
          style={{ border: '1px solid var(--border)', background: 'var(--bg-surface)' }}
        >
          <div className="flex items-center justify-between px-4 py-3" style={{ borderBottom: '1px solid var(--border)' }}>
            <p className="text-[11px] font-semibold uppercase tracking-[0.8px]" style={{ color: 'var(--text-3)' }}>
              {loading ? 'Loading…' : `${rows.length} consumer${rows.length !== 1 ? 's' : ''}`}
            </p>
            {!showForm && (
              <button
                onClick={() => setShowForm(true)}
                className="flex items-center gap-1.5 rounded-md border px-3 py-1.5 text-[12px] font-medium transition-colors hover:bg-[var(--bg-hover)]"
                style={{ borderColor: 'var(--border-mid)', color: 'var(--text-2)' }}
              >
                <Plus className="h-3.5 w-3.5" />
                Register Consumer
              </button>
            )}
          </div>
          {error ? (
            <div className="px-4 py-3 text-[12.5px]" style={{ color: 'var(--red)' }}>
              Failed to load consumers: {error}
            </div>
          ) : (
            <ConsumerTable
              rows={rows}
              onSelect={(id) => navigate(`/consumers/${id}`)}
              onRegister={() => setShowForm(true)}
            />
          )}
        </div>
      </div>
    </div>
  )
}

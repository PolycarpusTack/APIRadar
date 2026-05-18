import { useEffect, useState } from 'react'
import { useNavigate } from 'react-router-dom'
import { Server, Plus, X } from 'lucide-react'
import PageHeader from '../components/PageHeader'
import EmptyState from '../components/EmptyState'

interface ServiceRow {
  id: string
  name: string
  repo_url: string
  owner_team: string
  spec_format: string
}

const TABLE_COLS = ['Service', 'Team', 'Format', 'ID']

function RegisterForm({ onCreated }: { onCreated: (svc: ServiceRow) => void }) {
  const [open, setOpen] = useState(false)
  const [name, setName] = useState('')
  const [repoUrl, setRepoUrl] = useState('')
  const [ownerTeam, setOwnerTeam] = useState('')
  const [specFormat, setSpecFormat] = useState('openapi')
  const [saving, setSaving] = useState(false)
  const [err, setErr] = useState<string | null>(null)

  function reset() {
    setName(''); setRepoUrl(''); setOwnerTeam(''); setSpecFormat('openapi')
    setErr(null); setOpen(false)
  }

  async function submit(e: React.FormEvent) {
    e.preventDefault()
    setSaving(true); setErr(null)
    try {
      const resp = await fetch('/v1/services', {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({ name, repo_url: repoUrl, owner_team: ownerTeam, spec_format: specFormat }),
      })
      if (!resp.ok) {
        const body = await resp.json().catch(() => ({}))
        throw new Error((body as { error?: string }).error ?? `HTTP ${resp.status}`)
      }
      const svc = await resp.json() as ServiceRow
      onCreated(svc)
      reset()
    } catch (e) {
      setErr((e as Error).message)
    } finally {
      setSaving(false)
    }
  }

  if (!open) {
    return (
      <button
        onClick={() => setOpen(true)}
        className="flex items-center gap-1.5 rounded-md border px-3 py-1.5 text-[12px] font-medium transition-colors hover:bg-[var(--bg-hover)]"
        style={{ borderColor: 'var(--border-mid)', color: 'var(--text-2)' }}
      >
        <Plus className="h-3.5 w-3.5" />
        Register Service
      </button>
    )
  }

  return (
    <div
      className="rounded-lg border p-5 mb-6"
      style={{ border: '1px solid var(--border)', background: 'var(--bg-surface)' }}
    >
      <div className="flex items-center justify-between mb-4">
        <p className="text-[12.5px] font-semibold" style={{ color: 'var(--text-1)' }}>Register a new service</p>
        <button onClick={reset} style={{ color: 'var(--text-3)' }}><X className="h-4 w-4" /></button>
      </div>
      <form onSubmit={submit} className="grid grid-cols-2 gap-3">
        <div className="col-span-2 sm:col-span-1">
          <label className="block text-[10.5px] font-medium uppercase tracking-[0.8px] mb-1" style={{ color: 'var(--text-3)' }}>Name *</label>
          <input
            required
            value={name}
            onChange={e => setName(e.target.value)}
            className="w-full rounded border px-2.5 py-1.5 text-[12.5px] outline-none focus:ring-1"
            style={{ background: 'var(--bg-raised)', border: '1px solid var(--border)', color: 'var(--text-1)' }}
            placeholder="payments-api"
          />
        </div>
        <div className="col-span-2 sm:col-span-1">
          <label className="block text-[10.5px] font-medium uppercase tracking-[0.8px] mb-1" style={{ color: 'var(--text-3)' }}>Owner Team</label>
          <input
            value={ownerTeam}
            onChange={e => setOwnerTeam(e.target.value)}
            className="w-full rounded border px-2.5 py-1.5 text-[12.5px] outline-none focus:ring-1"
            style={{ background: 'var(--bg-raised)', border: '1px solid var(--border)', color: 'var(--text-1)' }}
            placeholder="platform"
          />
        </div>
        <div className="col-span-2 sm:col-span-1">
          <label className="block text-[10.5px] font-medium uppercase tracking-[0.8px] mb-1" style={{ color: 'var(--text-3)' }}>Repo URL</label>
          <input
            value={repoUrl}
            onChange={e => setRepoUrl(e.target.value)}
            className="w-full rounded border px-2.5 py-1.5 text-[12.5px] outline-none focus:ring-1"
            style={{ background: 'var(--bg-raised)', border: '1px solid var(--border)', color: 'var(--text-1)', fontFamily: 'var(--font-mono)' }}
            placeholder="https://github.com/org/repo"
          />
        </div>
        <div className="col-span-2 sm:col-span-1">
          <label className="block text-[10.5px] font-medium uppercase tracking-[0.8px] mb-1" style={{ color: 'var(--text-3)' }}>Spec Format</label>
          <select
            value={specFormat}
            onChange={e => setSpecFormat(e.target.value)}
            className="w-full rounded border px-2.5 py-1.5 text-[12.5px] outline-none"
            style={{ background: 'var(--bg-raised)', border: '1px solid var(--border)', color: 'var(--text-1)' }}
          >
            <option value="openapi">OpenAPI</option>
            <option value="graphql">GraphQL</option>
            <option value="protobuf">Protobuf</option>
          </select>
        </div>
        {err && (
          <p className="col-span-2 text-[12px]" style={{ color: 'var(--red)' }}>{err}</p>
        )}
        <div className="col-span-2 flex gap-2 pt-1">
          <button
            type="submit"
            disabled={saving}
            className="rounded-md px-3 py-1.5 text-[12px] font-semibold transition-colors"
            style={{ background: 'var(--cobalt)', color: 'var(--text-inverse)' }}
          >
            {saving ? 'Saving…' : 'Register'}
          </button>
          <button
            type="button"
            onClick={reset}
            className="rounded-md px-3 py-1.5 text-[12px] transition-colors hover:bg-[var(--bg-hover)]"
            style={{ color: 'var(--text-3)' }}
          >
            Cancel
          </button>
        </div>
      </form>
    </div>
  )
}

function ServiceTable({ rows, onSelect }: { rows: ServiceRow[]; onSelect: (id: string) => void }) {
  if (rows.length === 0) {
    return (
      <EmptyState
        icon={Server}
        title="No services registered"
        description="Register a service via the form above or use the CLI: radar check --base old.yaml --head new.yaml --service-id <id>"
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
                    onClick={e => e.stopPropagation()}
                    className="text-[11px] underline decoration-dotted hover:no-underline"
                    style={{ color: 'var(--cobalt-mid)', fontFamily: 'var(--font-mono)' }}
                  >
                    {row.repo_url.replace(/^https?:\/\//, '').slice(0, 40)}
                  </a>
                )}
              </td>
              <td className="px-3 py-2.5 group-hover:bg-[var(--bg-hover)]" style={{ fontSize: '12.5px', color: 'var(--text-2)' }}>
                {row.owner_team || <span style={{ color: 'var(--text-dim)' }}>—</span>}
              </td>
              <td className="px-3 py-2.5 group-hover:bg-[var(--bg-hover)]" style={{ fontFamily: 'var(--font-mono)', fontSize: '11px', color: 'var(--text-3)' }}>
                {row.spec_format}
              </td>
              <td className="px-3 py-2.5 group-hover:bg-[var(--bg-hover)]" style={{ fontFamily: 'var(--font-mono)', fontSize: '11px', color: 'var(--text-dim)' }}>
                {row.id.slice(0, 8)}…
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  )
}

export default function ServicesPage() {
  const navigate = useNavigate()
  const [rows, setRows] = useState<ServiceRow[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    fetch('/v1/services')
      .then((r) => {
        if (!r.ok) throw new Error(`HTTP ${r.status}`)
        return r.json() as Promise<ServiceRow[]>
      })
      .then(setRows)
      .catch((e: Error) => setError(e.message))
      .finally(() => setLoading(false))
  }, [])

  function handleCreated(svc: ServiceRow) {
    setRows((prev) => {
      const exists = prev.find(r => r.id === svc.id)
      return exists ? prev.map(r => r.id === svc.id ? svc : r) : [svc, ...prev]
    })
  }

  return (
    <div>
      <PageHeader
        tag="Registry"
        title="Services"
        description="Producer services registered with Radar. Click a row to view its diff history, or register a new service to start tracking schema changes."
      />

      <div className="px-14 py-8">
        <RegisterForm onCreated={handleCreated} />

        <div className="overflow-hidden rounded-lg" style={{ border: '1px solid var(--border)', background: 'var(--bg-surface)' }}>
          <div className="flex items-center px-4 py-3" style={{ borderBottom: '1px solid var(--border)' }}>
            <p className="text-[11px] font-semibold uppercase tracking-[0.8px]" style={{ color: 'var(--text-3)' }}>
              {loading ? 'Loading…' : `${rows.length} service${rows.length !== 1 ? 's' : ''}`}
            </p>
          </div>
          {error ? (
            <div className="px-4 py-3 text-[12.5px]" style={{ color: 'var(--red)' }}>
              Failed to load services: {error}
            </div>
          ) : (
            <ServiceTable rows={rows} onSelect={(id) => navigate(`/diffs?service=${id}`)} />
          )}
        </div>
      </div>
    </div>
  )
}

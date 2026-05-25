import { useEffect, useState } from 'react'
import { Database, RefreshCw, Plus, X } from 'lucide-react'
import PageHeader from '../components/PageHeader'
import Badge from '../components/Badge'
import EmptyState from '../components/EmptyState'

interface CatalogSource {
  id: string
  kind: string
  name: string
  url: string
  token_env: string | null
  sync_interval_secs: number
  last_sync_at: string | null
  last_sync_status: string | null
  last_sync_error: string | null
  created_at: string
}

interface SyncResult {
  source_id: string
  synced_at: string
  status: string
  consumers_upserted: number
  error: string | null
}

const KIND_LABELS: Record<string, string> = {
  backstage: 'Backstage',
  codeowners: 'CODEOWNERS',
  csv: 'CSV',
  manual: 'Manual',
}

function formatDate(iso: string) {
  try {
    return new Date(iso).toLocaleString('en-GB', {
      day: '2-digit', month: 'short', year: 'numeric', hour: '2-digit', minute: '2-digit',
    })
  } catch {
    return iso
  }
}

function syncStatusVariant(status: string | null): 'ok' | 'err' | 'neutral' {
  if (status === 'ok') return 'ok'
  if (status === 'error') return 'err'
  return 'neutral'
}

interface CreateFormState {
  kind: string
  name: string
  url: string
  token_env: string
  sync_interval_secs: string
}

const DEFAULT_FORM: CreateFormState = {
  kind: 'backstage',
  name: '',
  url: '',
  token_env: '',
  sync_interval_secs: '3600',
}

export default function CatalogSourcesPage() {
  const [sources, setSources] = useState<CatalogSource[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [syncing, setSyncing] = useState<Record<string, boolean>>({})
  const [syncResults, setSyncResults] = useState<Record<string, SyncResult>>({})
  const [showCreate, setShowCreate] = useState(false)
  const [form, setForm] = useState<CreateFormState>(DEFAULT_FORM)
  const [creating, setCreating] = useState(false)
  const [createError, setCreateError] = useState<string | null>(null)

  function loadSources() {
    setLoading(true)
    fetch('/v1/catalog-sources')
      .then((r) => {
        if (!r.ok) throw new Error(`HTTP ${r.status}`)
        return r.json() as Promise<{ entries: CatalogSource[] }>
      })
      .then((data) => setSources(data.entries ?? []))
      .catch((e: Error) => setError(e.message))
      .finally(() => setLoading(false))
  }

  useEffect(() => { loadSources() }, [])

  function handleSync(id: string) {
    setSyncing((s) => ({ ...s, [id]: true }))
    fetch(`/v1/catalog-sources/${id}/sync`, { method: 'POST' })
      .then((r) => {
        if (!r.ok) throw new Error(`HTTP ${r.status}`)
        return r.json() as Promise<SyncResult>
      })
      .then((result) => {
        setSyncResults((s) => ({ ...s, [id]: result }))
        loadSources()
      })
      .catch((e: Error) => {
        setSyncResults((s) => ({ ...s, [id]: { source_id: id, synced_at: '', status: 'error', consumers_upserted: 0, error: e.message } }))
      })
      .finally(() => setSyncing((s) => ({ ...s, [id]: false })))
  }

  function handleCreate(e: React.FormEvent) {
    e.preventDefault()
    setCreating(true)
    setCreateError(null)
    const body: Record<string, unknown> = {
      kind: form.kind,
      name: form.name,
      url: form.url || undefined,
      token_env: form.token_env || undefined,
      sync_interval_secs: form.sync_interval_secs ? Number(form.sync_interval_secs) : undefined,
    }
    fetch('/v1/catalog-sources', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(body),
    })
      .then((r) => {
        if (!r.ok) return r.text().then((t) => { throw new Error(t || `HTTP ${r.status}`) })
        return r.json()
      })
      .then(() => {
        setShowCreate(false)
        setForm(DEFAULT_FORM)
        loadSources()
      })
      .catch((e: Error) => setCreateError(e.message))
      .finally(() => setCreating(false))
  }

  return (
    <div>
      <PageHeader
        tag="Registry"
        title="Catalog Sources"
        description="Import consumer records automatically from Backstage, CODEOWNERS files, or CSV. Each source is polled on its configured schedule."
      />

      <div className="px-14 py-8 space-y-6">
        {/* Action bar */}
        <div className="flex justify-end">
          <button
            onClick={() => setShowCreate((v) => !v)}
            className="flex items-center gap-1.5 rounded-md px-3 py-1.5 text-[12px] font-medium transition-colors"
            style={{ background: 'var(--cobalt)', color: 'var(--text-inverse)' }}
          >
            <Plus className="h-3.5 w-3.5" />
            Add source
          </button>
        </div>

        {/* Create form */}
        {showCreate && (
          <div className="rounded-lg p-5" style={{ border: '1px solid var(--cobalt-muted)', background: 'var(--bg-surface)' }}>
            <div className="flex items-center justify-between mb-4">
              <p className="text-[13px] font-semibold" style={{ color: 'var(--text-1)' }}>New Catalog Source</p>
              <button onClick={() => { setShowCreate(false); setCreateError(null) }}>
                <X className="h-4 w-4" style={{ color: 'var(--text-3)' }} />
              </button>
            </div>
            <form onSubmit={handleCreate} className="grid grid-cols-2 gap-3">
              <div>
                <label className="block mb-1 text-[10.5px] font-semibold uppercase tracking-[0.8px]" style={{ color: 'var(--text-3)' }}>
                  Kind
                </label>
                <select
                  value={form.kind}
                  onChange={(e) => setForm((f) => ({ ...f, kind: e.target.value }))}
                  className="w-full rounded-md border px-2.5 py-1.5 text-[12.5px]"
                  style={{ borderColor: 'var(--border-mid)', background: 'var(--bg-raised)', color: 'var(--text-1)' }}
                >
                  <option value="backstage">Backstage</option>
                  <option value="codeowners">CODEOWNERS</option>
                  <option value="csv">CSV</option>
                  <option value="manual">Manual</option>
                </select>
              </div>
              <div>
                <label className="block mb-1 text-[10.5px] font-semibold uppercase tracking-[0.8px]" style={{ color: 'var(--text-3)' }}>
                  Name
                </label>
                <input
                  required
                  value={form.name}
                  onChange={(e) => setForm((f) => ({ ...f, name: e.target.value }))}
                  placeholder="Internal Backstage"
                  className="w-full rounded-md border px-2.5 py-1.5 text-[12.5px]"
                  style={{ borderColor: 'var(--border-mid)', background: 'var(--bg-raised)', color: 'var(--text-1)' }}
                />
              </div>
              <div className="col-span-2">
                <label className="block mb-1 text-[10.5px] font-semibold uppercase tracking-[0.8px]" style={{ color: 'var(--text-3)' }}>
                  URL
                </label>
                <input
                  value={form.url}
                  onChange={(e) => setForm((f) => ({ ...f, url: e.target.value }))}
                  placeholder="https://backstage.internal.example.com"
                  className="w-full rounded-md border px-2.5 py-1.5 text-[12.5px]"
                  style={{ borderColor: 'var(--border-mid)', background: 'var(--bg-raised)', color: 'var(--text-1)' }}
                />
              </div>
              <div>
                <label className="block mb-1 text-[10.5px] font-semibold uppercase tracking-[0.8px]" style={{ color: 'var(--text-3)' }}>
                  Token env var
                </label>
                <input
                  value={form.token_env}
                  onChange={(e) => setForm((f) => ({ ...f, token_env: e.target.value }))}
                  placeholder="BACKSTAGE_TOKEN"
                  className="w-full rounded-md border px-2.5 py-1.5 text-[12.5px]"
                  style={{ borderColor: 'var(--border-mid)', background: 'var(--bg-raised)', color: 'var(--text-1)' }}
                />
              </div>
              <div>
                <label className="block mb-1 text-[10.5px] font-semibold uppercase tracking-[0.8px]" style={{ color: 'var(--text-3)' }}>
                  Sync interval (secs)
                </label>
                <input
                  type="number"
                  value={form.sync_interval_secs}
                  onChange={(e) => setForm((f) => ({ ...f, sync_interval_secs: e.target.value }))}
                  placeholder="3600"
                  className="w-full rounded-md border px-2.5 py-1.5 text-[12.5px]"
                  style={{ borderColor: 'var(--border-mid)', background: 'var(--bg-raised)', color: 'var(--text-1)' }}
                />
              </div>
              {createError && (
                <p className="col-span-2 text-[12px]" style={{ color: 'var(--red)' }}>{createError}</p>
              )}
              <div className="col-span-2 flex justify-end gap-2 mt-1">
                <button
                  type="button"
                  onClick={() => { setShowCreate(false); setCreateError(null) }}
                  className="rounded-md px-3 py-1.5 text-[12px]"
                  style={{ border: '1px solid var(--border-mid)', color: 'var(--text-2)', background: 'var(--bg-raised)' }}
                >
                  Cancel
                </button>
                <button
                  type="submit"
                  disabled={creating}
                  className="rounded-md px-3 py-1.5 text-[12px] font-medium"
                  style={{ background: 'var(--cobalt)', color: 'var(--text-inverse)', opacity: creating ? 0.6 : 1 }}
                >
                  {creating ? 'Creating…' : 'Create'}
                </button>
              </div>
            </form>
          </div>
        )}

        {/* Sources list */}
        <div className="overflow-hidden rounded-lg" style={{ border: '1px solid var(--border)', background: 'var(--bg-surface)' }}>
          <div className="flex items-center px-4 py-3" style={{ borderBottom: '1px solid var(--border)' }}>
            <p className="text-[11px] font-semibold uppercase tracking-[0.8px]" style={{ color: 'var(--text-3)' }}>
              {loading ? 'Loading…' : `${sources.length} source${sources.length !== 1 ? 's' : ''}`}
            </p>
          </div>

          {error ? (
            <div className="px-4 py-3 text-[12.5px]" style={{ color: 'var(--red)' }}>
              Failed to load catalog sources: {error}
            </div>
          ) : sources.length === 0 && !loading ? (
            <EmptyState
              icon={Database}
              title="No catalog sources configured"
              description="Add a Backstage or CODEOWNERS source to automatically import consumer records and enrich blast-radius results with ownership data."
            />
          ) : (
            <table className="w-full border-collapse">
              <thead>
                <tr>
                  {['Kind', 'Name', 'URL', 'Last Sync', 'Status', ''].map((col) => (
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
                {sources.map((src) => {
                  const result = syncResults[src.id]
                  return (
                    <tr key={src.id} className="group" style={{ borderBottom: '1px solid var(--border)' }}>
                      <td className="px-3 py-2.5 group-hover:bg-[var(--bg-hover)]">
                        <Badge variant="cobalt">{KIND_LABELS[src.kind] ?? src.kind}</Badge>
                      </td>
                      <td
                        className="px-3 py-2.5 font-medium group-hover:bg-[var(--bg-hover)]"
                        style={{ fontSize: '12.5px', color: 'var(--text-1)' }}
                      >
                        {src.name}
                      </td>
                      <td
                        className="px-3 py-2.5 group-hover:bg-[var(--bg-hover)]"
                        style={{ fontFamily: 'var(--font-mono)', fontSize: '11.5px', color: 'var(--text-3)', maxWidth: '200px' }}
                      >
                        <span className="truncate block" title={src.url}>{src.url || <span style={{ color: 'var(--text-dim)' }}>—</span>}</span>
                      </td>
                      <td
                        className="px-3 py-2.5 group-hover:bg-[var(--bg-hover)]"
                        style={{ fontFamily: 'var(--font-mono)', fontSize: '11.5px', color: 'var(--text-3)' }}
                      >
                        {src.last_sync_at ? formatDate(src.last_sync_at) : <span style={{ color: 'var(--text-dim)' }}>never</span>}
                      </td>
                      <td className="px-3 py-2.5 group-hover:bg-[var(--bg-hover)]">
                        <div className="space-y-1">
                          {src.last_sync_status && (
                            <Badge variant={syncStatusVariant(src.last_sync_status)}>
                              {src.last_sync_status === 'ok' ? 'synced' : src.last_sync_status}
                            </Badge>
                          )}
                          {result && (
                            <p className="text-[11px]" style={{ color: result.status === 'ok' ? 'var(--green)' : 'var(--red)', fontFamily: 'var(--font-mono)' }}>
                              {result.status === 'ok'
                                ? `+${result.consumers_upserted} consumers`
                                : result.error ?? 'sync failed'}
                            </p>
                          )}
                          {src.last_sync_error && !result && (
                            <p className="text-[11px] truncate max-w-[160px]" title={src.last_sync_error} style={{ color: 'var(--red)', fontFamily: 'var(--font-mono)' }}>
                              {src.last_sync_error}
                            </p>
                          )}
                        </div>
                      </td>
                      <td className="px-3 py-2.5 group-hover:bg-[var(--bg-hover)]">
                        <button
                          onClick={() => handleSync(src.id)}
                          disabled={syncing[src.id]}
                          className="flex items-center gap-1 rounded-md border px-2.5 py-1 text-[11.5px] font-medium transition-colors hover:bg-[var(--bg-hover)]"
                          style={{ borderColor: 'var(--border-mid)', color: 'var(--text-2)', opacity: syncing[src.id] ? 0.6 : 1 }}
                        >
                          <RefreshCw className={`h-3 w-3 ${syncing[src.id] ? 'animate-spin' : ''}`} />
                          {syncing[src.id] ? 'Syncing…' : 'Sync now'}
                        </button>
                      </td>
                    </tr>
                  )
                })}
              </tbody>
            </table>
          )}
        </div>
      </div>
    </div>
  )
}

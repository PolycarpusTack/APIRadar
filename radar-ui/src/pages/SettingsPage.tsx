import { useEffect, useState } from 'react'
import { CheckCircle, XCircle, Webhook, Trash2, Send, ChevronDown, ChevronUp, ScanLine, Plus, AlertTriangle, Clock, RefreshCw, Download } from 'lucide-react'
import PageHeader from '../components/PageHeader'
import TermTooltip, { TERM_DEFINITIONS } from '../components/TermTooltip'
import { api, ApiError } from '../lib/apiClient'

interface AppSettings {
  policy_block_on: string
  policy_lookback_days: number
  policy_allow_override_with: string | null
  retention_days: number
}

interface Integrations {
  anthropic: boolean
  openai: boolean
  openai_enterprise: boolean
  github_copilot: boolean
  jira: boolean
  github: boolean
  postman: boolean
}

const DEFAULTS: AppSettings = {
  policy_block_on: 'active_consumers',
  policy_lookback_days: 30,
  policy_allow_override_with: null,
  retention_days: 90,
}

function SectionCard({ title, description, children }: { title: string; description: string; children: React.ReactNode }) {
  return (
    <div
      className="rounded-lg border mb-6"
      style={{ border: '1px solid var(--border)', background: 'var(--bg-surface)' }}
    >
      <div className="px-6 py-4" style={{ borderBottom: '1px solid var(--border)' }}>
        <p className="text-[13px] font-semibold" style={{ color: 'var(--text-1)' }}>{title}</p>
        <p className="text-[12px] mt-0.5" style={{ color: 'var(--text-3)' }}>{description}</p>
      </div>
      <div className="px-6 py-5 space-y-4">{children}</div>
    </div>
  )
}

function FieldRow({ label, hint, tooltip, children }: { label: string; hint?: string; tooltip?: keyof typeof TERM_DEFINITIONS; children: React.ReactNode }) {
  return (
    <div className="grid grid-cols-[200px_1fr] items-start gap-6">
      <div>
        <p className="flex items-center gap-1 text-[11.5px] font-medium" style={{ color: 'var(--text-2)' }}>
          {label}
          {tooltip && <TermTooltip term={tooltip} placement="bottom" />}
        </p>
        {hint && <p className="text-[11px] mt-0.5 leading-snug" style={{ color: 'var(--text-dim)' }}>{hint}</p>}
      </div>
      <div>{children}</div>
    </div>
  )
}

function IntegrationChip({ label, active }: { label: string; active: boolean }) {
  return (
    <div className="flex items-center gap-2 py-1">
      {active
        ? <CheckCircle className="h-3.5 w-3.5 flex-shrink-0" style={{ color: 'var(--green)' }} />
        : <XCircle    className="h-3.5 w-3.5 flex-shrink-0" style={{ color: 'var(--text-dim)' }} />
      }
      <span className="text-[12.5px]" style={{ color: active ? 'var(--text-1)' : 'var(--text-3)' }}>{label}</span>
      <span
        className="ml-1 rounded px-1.5 py-px text-[10px] font-medium"
        style={{
          background: active ? 'var(--bg-active)' : 'var(--bg-raised)',
          color: active ? 'var(--cobalt-mid)' : 'var(--text-dim)',
        }}
      >
        {active ? 'configured' : 'not set'}
      </span>
    </div>
  )
}

// ---------------------------------------------------------------------------
// Webhook types
// ---------------------------------------------------------------------------

interface WebhookEntry {
  id: string
  url: string
  events: string[]
  secret?: string
  secret_hint: string
  active: boolean
  created_at: string
}

interface DeliveryEntry {
  id: string
  event: string
  status: string
  attempt: number
  error?: string
  delivered_at?: string
}

const ALL_EVENTS = ['diff.created']

function WebhooksSection() {
  const [webhooks, setWebhooks] = useState<WebhookEntry[]>([])
  const [loading, setLoading] = useState(true)
  const [newUrl, setNewUrl] = useState('')
  const [newEvents, setNewEvents] = useState(['diff.created'])
  const [creating, setCreating] = useState(false)
  const [createError, setCreateError] = useState<string | null>(null)
  const [newSecret, setNewSecret] = useState<string | null>(null)
  const [expandedId, setExpandedId] = useState<string | null>(null)
  const [deliveries, setDeliveries] = useState<Record<string, DeliveryEntry[]>>({})

  const reload = () => {
    setLoading(true)
    api.get<WebhookEntry[]>('/v1/webhooks')
      .then(setWebhooks)
      .catch(() => {})
      .finally(() => setLoading(false))
  }

  useEffect(reload, [])

  async function createWebhook(e: React.FormEvent) {
    e.preventDefault()
    if (!newUrl) return
    setCreating(true); setCreateError(null); setNewSecret(null)
    try {
      const body = await api.post<WebhookEntry>('/v1/webhooks', { url: newUrl, events: newEvents })
      if (body.secret) setNewSecret(body.secret ?? null)
      setNewUrl('')
      reload()
    } catch (err) {
      setCreateError(err instanceof ApiError ? (err.body as { error?: string })?.error ?? err.message : (err as Error).message)
    } finally {
      setCreating(false)
    }
  }

  async function deleteWebhook(id: string) {
    await api.del(`/v1/webhooks/${id}`)
    setWebhooks(prev => prev.filter(w => w.id !== id))
    if (expandedId === id) setExpandedId(null)
  }

  async function testWebhook(id: string) {
    await api.post(`/v1/webhooks/${id}/test`)
  }

  async function toggleExpand(id: string) {
    if (expandedId === id) { setExpandedId(null); return }
    setExpandedId(id)
    if (!deliveries[id]) {
      try {
        const data = await api.get<DeliveryEntry[]>(`/v1/webhooks/${id}/deliveries`)
        setDeliveries(prev => ({ ...prev, [id]: data }))
      } catch {
        // silently ignore delivery load failures
      }
    }
  }

  const statusColor = (s: string) => s === 'delivered' ? 'var(--green)' : s === 'failed' ? 'var(--red)' : 'var(--text-dim)'

  return (
    <SectionCard title="Webhooks" description="Register HTTP callbacks to receive push notifications when diffs are created.">
      {loading ? (
        <p className="text-[12px]" style={{ color: 'var(--text-dim)' }}>Loading…</p>
      ) : (
        <div className="space-y-3">
          {webhooks.length === 0 && (
            <p className="text-[12px]" style={{ color: 'var(--text-dim)' }}>No webhooks registered yet.</p>
          )}
          {webhooks.map(wh => (
            <div key={wh.id} className="rounded border" style={{ border: '1px solid var(--border)', background: 'var(--bg-raised)' }}>
              <div className="flex items-center gap-3 px-3 py-2">
                <Webhook className="h-3.5 w-3.5 flex-shrink-0" style={{ color: 'var(--text-dim)' }} />
                <span className="flex-1 text-[12px] font-mono truncate" style={{ color: 'var(--text-1)' }}>{wh.url}</span>
                <span className="text-[10.5px] rounded px-1.5 py-px" style={{ background: 'var(--bg-active)', color: 'var(--text-dim)' }}>
                  {wh.events.join(', ')}
                </span>
                <span className="text-[10.5px]" style={{ color: 'var(--text-dim)' }}>secret: {wh.secret_hint}</span>
                <button
                  onClick={() => testWebhook(wh.id)}
                  title="Send test ping"
                  className="rounded p-1 transition-colors hover:opacity-70"
                  style={{ color: 'var(--cobalt-mid)' }}
                >
                  <Send className="h-3 w-3" />
                </button>
                <button
                  onClick={() => deleteWebhook(wh.id)}
                  title="Delete webhook"
                  className="rounded p-1 transition-colors hover:opacity-70"
                  style={{ color: 'var(--red)' }}
                >
                  <Trash2 className="h-3 w-3" />
                </button>
                <button
                  onClick={() => toggleExpand(wh.id)}
                  className="rounded p-1 transition-colors hover:opacity-70"
                  style={{ color: 'var(--text-dim)' }}
                >
                  {expandedId === wh.id ? <ChevronUp className="h-3 w-3" /> : <ChevronDown className="h-3 w-3" />}
                </button>
              </div>
              {expandedId === wh.id && (
                <div className="px-3 pb-3">
                  <p className="text-[10.5px] font-semibold uppercase tracking-[0.7px] mb-1.5" style={{ color: 'var(--text-dim)' }}>Recent deliveries</p>
                  {(deliveries[wh.id] ?? []).length === 0 ? (
                    <p className="text-[11.5px]" style={{ color: 'var(--text-dim)' }}>No deliveries yet.</p>
                  ) : (deliveries[wh.id] ?? []).map(d => (
                    <div key={d.id} className="flex items-center gap-3 py-0.5">
                      <span className="text-[11px] font-medium w-20" style={{ color: statusColor(d.status) }}>{d.status}</span>
                      <span className="text-[11px] font-mono" style={{ color: 'var(--text-2)' }}>{d.event}</span>
                      <span className="text-[10.5px]" style={{ color: 'var(--text-dim)' }}>attempt {d.attempt}</span>
                      {d.error && <span className="text-[10.5px]" style={{ color: 'var(--red)' }}>{d.error}</span>}
                    </div>
                  ))}
                </div>
              )}
            </div>
          ))}

          {newSecret && (
            <div className="rounded border px-3 py-2 text-[12px]" style={{ border: '1px solid var(--green)', background: 'var(--bg-raised)', color: 'var(--green)' }}>
              Webhook created. Secret (shown once): <span className="font-mono">{newSecret}</span>
              <button className="ml-3 underline text-[11px]" onClick={() => setNewSecret(null)}>dismiss</button>
            </div>
          )}

          <form onSubmit={createWebhook} className="flex items-end gap-2 pt-1">
            <div className="flex-1">
              <label className="text-[11px] font-medium block mb-1" style={{ color: 'var(--text-2)' }}>HTTPS endpoint URL</label>
              <input
                type="url"
                value={newUrl}
                onChange={e => setNewUrl(e.target.value)}
                placeholder="https://hooks.example.com/radar"
                className="w-full rounded border px-2.5 py-1.5 text-[12.5px] outline-none focus:ring-1 font-mono"
                style={{ background: 'var(--bg-raised)', border: '1px solid var(--border)', color: 'var(--text-1)' }}
                required
              />
            </div>
            <div>
              <label className="text-[11px] font-medium block mb-1" style={{ color: 'var(--text-2)' }}>Events</label>
              <select
                value={newEvents[0]}
                onChange={e => setNewEvents([e.target.value])}
                className="rounded border px-2 py-1.5 text-[12px] outline-none"
                style={{ background: 'var(--bg-raised)', border: '1px solid var(--border)', color: 'var(--text-1)' }}
              >
                {ALL_EVENTS.map(ev => <option key={ev} value={ev}>{ev}</option>)}
              </select>
            </div>
            <button
              type="submit"
              disabled={creating}
              className="rounded-md px-3 py-1.5 text-[12px] font-semibold"
              style={{ background: 'var(--cobalt)', color: 'var(--text-inverse)', opacity: creating ? 0.7 : 1 }}
            >
              {creating ? 'Registering…' : 'Register'}
            </button>
          </form>
          {createError && <p className="text-[12px]" style={{ color: 'var(--red)' }}>{createError}</p>}
        </div>
      )}
    </SectionCard>
  )
}

// ---------------------------------------------------------------------------
// Scalar update
// ---------------------------------------------------------------------------

interface ScalarVersionInfo {
  bundled: string
  override: string | null
  active: string
  latest: string | null
  update_available: boolean
}

function ScalarUpdateSection() {
  const [versionInfo, setVersionInfo] = useState<ScalarVersionInfo | null>(null)
  const [checking, setChecking] = useState(false)
  const [updating, setUpdating] = useState(false)
  const [updateResult, setUpdateResult] = useState<{ version: string } | null>(null)
  const [error, setError] = useState<string | null>(null)

  async function checkVersion() {
    setChecking(true); setError(null); setUpdateResult(null)
    try {
      const data = await api.get<ScalarVersionInfo>('/scalar/version')
      setVersionInfo(data)
    } catch (err) {
      setError(err instanceof ApiError ? err.message : String(err))
    } finally {
      setChecking(false)
    }
  }

  async function doUpdate() {
    setUpdating(true); setError(null)
    try {
      const data = await api.post<{ version: string; bytes: number }>('/scalar/update')
      setUpdateResult({ version: data.version })
      setVersionInfo(prev =>
        prev
          ? { ...prev, active: data.version, update_available: false, override: data.version }
          : prev
      )
    } catch (err) {
      setError(
        err instanceof ApiError
          ? (err.body as { error?: string })?.error ?? err.message
          : String(err)
      )
    } finally {
      setUpdating(false)
    }
  }

  return (
    <SectionCard
      title="API Explorer (Scalar)"
      description="The embedded Scalar bundle powers the Playground tab. Check npm for updates and apply them without rebuilding the application."
    >
      <FieldRow label="Active version" hint="Version of @scalar/api-reference currently served to the Playground.">
        <div className="flex items-center gap-2">
          <span className="text-[12.5px] font-mono" style={{ color: 'var(--text-1)' }}>
            {versionInfo ? versionInfo.active : '—'}
          </span>
          {versionInfo?.override && (
            <span
              className="rounded px-1.5 py-px text-[10.5px] font-medium"
              style={{ background: 'var(--bg-active)', color: 'var(--cobalt-mid)' }}
            >
              override active
            </span>
          )}
        </div>
      </FieldRow>

      <div className="flex items-center gap-3 flex-wrap">
        <button
          type="button"
          onClick={checkVersion}
          disabled={checking || updating}
          className="flex items-center gap-1.5 rounded-md border px-3 py-1.5 text-[12px] font-medium transition-colors hover:bg-[var(--bg-hover)]"
          style={{ borderColor: 'var(--border-mid)', color: 'var(--text-2)', opacity: checking ? 0.7 : 1 }}
        >
          <RefreshCw className={`h-3.5 w-3.5${checking ? ' animate-spin' : ''}`} />
          {checking ? 'Checking…' : 'Check for updates'}
        </button>

        {versionInfo?.update_available && (
          <button
            type="button"
            onClick={doUpdate}
            disabled={updating}
            className="flex items-center gap-1.5 rounded-md px-3 py-1.5 text-[12px] font-semibold transition-colors"
            style={{ background: 'var(--cobalt)', color: 'var(--text-inverse)', opacity: updating ? 0.7 : 1 }}
          >
            <Download className="h-3.5 w-3.5" />
            {updating ? 'Updating…' : `Update to ${versionInfo.latest}`}
          </button>
        )}
      </div>

      {versionInfo && !versionInfo.update_available && !updateResult && (
        <p className="text-[12px]" style={{ color: 'var(--green)' }}>
          ✓ Already on the latest version
          {versionInfo.latest ? ` (${versionInfo.latest})` : ''}.
          {' '}Bundled: {versionInfo.bundled}.
        </p>
      )}

      {updateResult && (
        <p className="text-[12px]" style={{ color: 'var(--green)' }}>
          ✓ Updated to {updateResult.version}. Open the Playground to use the new version — no restart needed.
        </p>
      )}

      {error && (
        <p className="text-[12px]" style={{ color: 'var(--red)' }}>{error}</p>
      )}
    </SectionCard>
  )
}

// ---------------------------------------------------------------------------
// Scheduled scans
// ---------------------------------------------------------------------------

interface ScanEntry {
  id: string
  service_id: string
  spec_url: string
  format: string
  interval_minutes: number
  last_run_at: string | null
  last_run_status: string | null
  last_run_error: string | null
  active: boolean
  created_at: string
}

function ScanStatusBadge({ status, error }: { status: string | null; error: string | null }) {
  if (!status || status === 'running') {
    return (
      <span className="inline-flex items-center gap-1 rounded px-1.5 py-px text-[10.5px]" style={{ background: 'var(--bg-raised)', color: 'var(--text-dim)' }}>
        <Clock className="h-3 w-3" /> {status === 'running' ? 'running' : 'pending'}
      </span>
    )
  }
  if (status === 'ok') {
    return (
      <span className="inline-flex items-center gap-1 rounded px-1.5 py-px text-[10.5px]" style={{ background: 'color-mix(in srgb, var(--teal) 12%, transparent)', color: 'var(--teal)' }}>
        <CheckCircle className="h-3 w-3" /> ok
      </span>
    )
  }
  return (
    <span
      className="inline-flex items-center gap-1 rounded px-1.5 py-px text-[10.5px]"
      style={{ background: 'color-mix(in srgb, var(--red) 10%, transparent)', color: 'var(--red)' }}
      title={error ?? undefined}
    >
      <AlertTriangle className="h-3 w-3" /> {status}
    </span>
  )
}

function ScheduledScansSection() {
  const [scans, setScans] = useState<ScanEntry[]>([])
  const [loading, setLoading] = useState(true)
  const [showForm, setShowForm] = useState(false)
  const [serviceId, setServiceId] = useState('')
  const [specUrl, setSpecUrl] = useState('')
  const [format, setFormat] = useState('openapi')
  const [intervalMinutes, setIntervalMinutes] = useState(60)
  const [creating, setCreating] = useState(false)
  const [createError, setCreateError] = useState<string | null>(null)

  const reload = () => {
    setLoading(true)
    api.get<ScanEntry[]>('/v1/scheduled-scans')
      .then(setScans)
      .catch(() => {})
      .finally(() => setLoading(false))
  }

  useEffect(reload, [])

  async function createScan(e: React.FormEvent) {
    e.preventDefault()
    setCreating(true); setCreateError(null)
    try {
      await api.post('/v1/scheduled-scans', { service_id: serviceId, spec_url: specUrl, format, interval_minutes: intervalMinutes })
      setShowForm(false); setServiceId(''); setSpecUrl('')
      reload()
    } catch (err) {
      setCreateError(err instanceof ApiError ? err.message : String(err))
    } finally {
      setCreating(false)
    }
  }

  async function deleteScan(id: string) {
    await api.del(`/v1/scheduled-scans/${id}`)
    setScans(prev => prev.filter(s => s.id !== id))
  }

  const inputCls = 'w-full rounded border px-2.5 py-1.5 text-[12.5px] outline-none focus:ring-1'
  const inputStyle = { background: 'var(--bg-raised)', border: '1px solid var(--border)', color: 'var(--text-1)' }

  return (
    <SectionCard title="Scheduled Scans" description="Automatically fetch and diff a spec URL on a recurring schedule. Requires an HTTPS endpoint that returns the raw spec.">
      {loading ? (
        <p className="text-[12px]" style={{ color: 'var(--text-dim)' }}>Loading…</p>
      ) : (
        <div className="space-y-3">
          {scans.length === 0 && !showForm && (
            <p className="text-[12px]" style={{ color: 'var(--text-dim)' }}>No scheduled scans configured.</p>
          )}
          {scans.map(s => (
            <div key={s.id} className="rounded border px-3 py-2 flex items-start gap-3" style={{ border: '1px solid var(--border)', background: 'var(--bg-raised)' }}>
              <ScanLine className="h-3.5 w-3.5 mt-0.5 flex-shrink-0" style={{ color: 'var(--text-dim)' }} />
              <div className="flex-1 min-w-0">
                <div className="flex items-center gap-2 flex-wrap">
                  <span className="text-[12px] font-mono truncate" style={{ color: 'var(--text-1)' }}>{s.spec_url}</span>
                  <span className="text-[10.5px] rounded px-1 py-px" style={{ background: 'var(--bg-active)', color: 'var(--text-dim)' }}>{s.format}</span>
                  <span className="text-[10.5px]" style={{ color: 'var(--text-dim)' }}>every {s.interval_minutes}m</span>
                  <ScanStatusBadge status={s.last_run_status} error={s.last_run_error} />
                </div>
                <p className="text-[10.5px] mt-0.5" style={{ color: 'var(--text-dim)', fontFamily: 'var(--font-mono)' }}>
                  service: {s.service_id}{s.last_run_at && ` · last run: ${new Date(s.last_run_at).toLocaleString()}`}
                </p>
                {s.last_run_error && (
                  <p className="text-[10.5px] mt-0.5 truncate" style={{ color: 'var(--red)' }}>{s.last_run_error}</p>
                )}
              </div>
              <button onClick={() => deleteScan(s.id)} title="Delete scan" className="rounded p-1 transition-colors hover:opacity-70 flex-shrink-0" style={{ color: 'var(--red)' }}>
                <Trash2 className="h-3 w-3" />
              </button>
            </div>
          ))}

          {showForm ? (
            <form onSubmit={createScan} className="rounded border p-3 space-y-2" style={{ border: '1px solid var(--border)', background: 'var(--bg-raised)' }}>
              <div className="grid grid-cols-2 gap-2">
                <div>
                  <label className="text-[11px] font-medium block mb-1" style={{ color: 'var(--text-2)' }}>Service ID</label>
                  <input value={serviceId} onChange={e => setServiceId(e.target.value)} placeholder="uuid" className={inputCls} style={inputStyle} required />
                </div>
                <div>
                  <label className="text-[11px] font-medium block mb-1" style={{ color: 'var(--text-2)' }}>Spec URL (HTTPS)</label>
                  <input type="url" value={specUrl} onChange={e => setSpecUrl(e.target.value)} placeholder="https://api.example.com/openapi.json" className={inputCls} style={inputStyle} required />
                </div>
                <div>
                  <label className="text-[11px] font-medium block mb-1" style={{ color: 'var(--text-2)' }}>Format</label>
                  <select value={format} onChange={e => setFormat(e.target.value)} className={inputCls} style={inputStyle}>
                    <option value="openapi">OpenAPI</option>
                    <option value="graphql">GraphQL</option>
                    <option value="protobuf">Protobuf</option>
                  </select>
                </div>
                <div>
                  <label className="text-[11px] font-medium block mb-1" style={{ color: 'var(--text-2)' }}>Interval (minutes, ≥15)</label>
                  <input type="number" min={15} value={intervalMinutes} onChange={e => setIntervalMinutes(Number(e.target.value))} className={inputCls} style={inputStyle} />
                </div>
              </div>
              {createError && <p className="text-[11.5px]" style={{ color: 'var(--red)' }}>{createError}</p>}
              <div className="flex items-center gap-2">
                <button type="submit" disabled={creating} className="rounded-md px-3 py-1.5 text-[12px] font-semibold" style={{ background: 'var(--cobalt)', color: 'var(--text-inverse)', opacity: creating ? 0.7 : 1 }}>
                  {creating ? 'Saving…' : 'Create'}
                </button>
                <button type="button" onClick={() => { setShowForm(false); setCreateError(null) }} className="rounded-md px-3 py-1.5 text-[12px]" style={{ color: 'var(--text-3)' }}>
                  Cancel
                </button>
              </div>
            </form>
          ) : (
            <button
              onClick={() => setShowForm(true)}
              className="flex items-center gap-1.5 rounded-md border px-3 py-1.5 text-[12px] font-medium transition-colors hover:bg-[var(--bg-hover)]"
              style={{ borderColor: 'var(--border-mid)', color: 'var(--text-2)' }}
            >
              <Plus className="h-3.5 w-3.5" />
              Add scheduled scan
            </button>
          )}
        </div>
      )}
    </SectionCard>
  )
}

// ---------------------------------------------------------------------------

export default function SettingsPage() {
  const [form, setForm] = useState<AppSettings>(DEFAULTS)
  const [integrations, setIntegrations] = useState<Integrations | null>(null)
  const [loading, setLoading] = useState(true)
  const [saving, setSaving] = useState(false)
  const [saved, setSaved] = useState(false)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    Promise.all([
      api.get<AppSettings>('/v1/settings'),
      api.get<Integrations>('/v1/settings/integrations'),
    ])
      .then(([s, i]) => { setForm(s); setIntegrations(i) })
      .catch((e: unknown) => setError(String(e)))
      .finally(() => setLoading(false))
  }, [])

  function set<K extends keyof AppSettings>(key: K, value: AppSettings[K]) {
    setForm(prev => ({ ...prev, [key]: value }))
    setSaved(false)
  }

  async function save(e: React.FormEvent) {
    e.preventDefault()
    setSaving(true); setError(null); setSaved(false)
    try {
      await api.put('/v1/settings', form)
      setSaved(true)
    } catch (e) {
      setError(e instanceof ApiError ? (e.body as { error?: string })?.error ?? e.message : (e as Error).message)
    } finally {
      setSaving(false)
    }
  }

  const inputCls = 'w-full rounded border px-2.5 py-1.5 text-[12.5px] outline-none focus:ring-1'
  const inputStyle = { background: 'var(--bg-raised)', border: '1px solid var(--border)', color: 'var(--text-1)' }

  return (
    <div>
      <PageHeader
        tag="Configuration"
        title="Settings"
        description="Policy rules, data retention, and integration status for this Radar instance."
      />

      <div className="px-14 py-8 max-w-3xl">
        {loading ? (
          <p className="text-[12.5px]" style={{ color: 'var(--text-3)' }}>Loading…</p>
        ) : (
          <form onSubmit={save}>
            <SectionCard
              title="Default Policy"
              description="Controls when a diff blocks CI and how long consumer activity is considered active."
            >
              <FieldRow
                label="Block on"
                hint="When to fail the CI check for a diff."
              >
                <select
                  value={form.policy_block_on}
                  onChange={e => set('policy_block_on', e.target.value)}
                  className={inputCls}
                  style={inputStyle}
                >
                  <option value="active_consumers">active_consumers — block only when active consumers are affected</option>
                  <option value="any_break">any_break — block on any breaking change</option>
                  <option value="never">never — warn only, never block</option>
                </select>
              </FieldRow>

              <FieldRow
                label="Lookback window"
                hint="Days of usage history used to determine if a consumer is active."
                tooltip="lookback_window"
              >
                <div className="flex items-center gap-2">
                  <input
                    type="number"
                    min={1}
                    max={365}
                    value={form.policy_lookback_days}
                    onChange={e => set('policy_lookback_days', Number(e.target.value))}
                    className={inputCls}
                    style={{ ...inputStyle, width: '100px' }}
                  />
                  <span className="text-[12px]" style={{ color: 'var(--text-3)' }}>days</span>
                </div>
              </FieldRow>

              <FieldRow
                label="Override label"
                hint="GitHub PR label that allows a diff to bypass blocking (leave blank to disable)."
              >
                <input
                  type="text"
                  value={form.policy_allow_override_with ?? ''}
                  onChange={e => set('policy_allow_override_with', e.target.value || null)}
                  className={inputCls}
                  style={{ ...inputStyle, fontFamily: 'var(--font-mono)' }}
                  placeholder="radar-approved"
                />
              </FieldRow>
            </SectionCard>

            <SectionCard
              title="Data Retention"
              description="Usage events older than this threshold are purged automatically once per hour."
            >
              <FieldRow label="Retain usage events for">
                <div className="flex items-center gap-2">
                  <input
                    type="number"
                    min={1}
                    max={3650}
                    value={form.retention_days}
                    onChange={e => set('retention_days', Number(e.target.value))}
                    className={inputCls}
                    style={{ ...inputStyle, width: '100px' }}
                  />
                  <span className="text-[12px]" style={{ color: 'var(--text-3)' }}>days</span>
                </div>
              </FieldRow>
            </SectionCard>

            <SectionCard
              title="Integrations"
              description="Read-only. Configure these by setting the corresponding environment variables on the server."
            >
              {integrations ? (
                <div className="space-y-3">
                  <div>
                    <p className="text-[10.5px] font-semibold uppercase tracking-[0.8px] mb-1.5" style={{ color: 'var(--text-dim)' }}>AI Providers</p>
                    <div className="space-y-1">
                      <IntegrationChip label="Anthropic — ANTHROPIC_API_KEY"                          active={integrations.anthropic} />
                      <IntegrationChip label="OpenAI — OPENAI_API_KEY"                                active={integrations.openai} />
                      <IntegrationChip label="ChatGPT Enterprise — OPENAI_API_KEY + OPENAI_BASE_URL"  active={integrations.openai_enterprise} />
                      <IntegrationChip label="GitHub Copilot — GITHUB_COPILOT_TOKEN"                  active={integrations.github_copilot} />
                    </div>
                  </div>
                  <div>
                    <p className="text-[10.5px] font-semibold uppercase tracking-[0.8px] mb-1.5" style={{ color: 'var(--text-dim)' }}>Dev Tools</p>
                    <div className="space-y-1">
                      <IntegrationChip label="GitHub — GITHUB_TOKEN"                                  active={integrations.github} />
                      <IntegrationChip label="Jira — JIRA_BASE_URL + JIRA_EMAIL + JIRA_TOKEN"         active={integrations.jira} />
                      <IntegrationChip label="Postman — POSTMAN_API_KEY"                              active={integrations.postman} />
                    </div>
                  </div>
                </div>
              ) : (
                <p className="text-[12px]" style={{ color: 'var(--text-dim)' }}>Unable to load integration status.</p>
              )}
            </SectionCard>

            <ScalarUpdateSection />
            <ScheduledScansSection />
            <WebhooksSection />

            {error && (
              <p className="mb-4 text-[12.5px]" style={{ color: 'var(--red)' }}>{error}</p>
            )}

            <div className="flex items-center gap-3">
              <button
                type="submit"
                disabled={saving}
                className="rounded-md px-4 py-2 text-[12.5px] font-semibold transition-colors"
                style={{ background: 'var(--cobalt)', color: 'var(--text-inverse)', opacity: saving ? 0.7 : 1 }}
              >
                {saving ? 'Saving…' : 'Save settings'}
              </button>
              {saved && (
                <span className="flex items-center gap-1.5 text-[12px]" style={{ color: 'var(--green)' }}>
                  <CheckCircle className="h-3.5 w-3.5" />
                  Saved
                </span>
              )}
            </div>
          </form>
        )}
      </div>
    </div>
  )
}

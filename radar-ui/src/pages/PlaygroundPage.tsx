import { useState, useCallback, useEffect, useRef } from 'react'
import {
  Telescope, ChevronDown, ChevronUp, Plus, Trash2,
  Database, Cloud, HardDrive, Loader2, Check, X, Rows,
} from 'lucide-react'
import PageHeader from '../components/PageHeader'
import CsvRunnerPanel from '../components/CsvRunnerPanel'

const API = ((import.meta as { env?: { VITE_API_URL?: string } }).env?.VITE_API_URL ?? '') + '/v1'
const DEFAULT_SPEC = 'https://cdn.jsdelivr.net/npm/@scalar/galaxy/dist/latest.yaml'
const LOCAL_STORAGE_KEY = 'drift-playground-envs-local'
// Mirrors --bg-base token (#0B0F19). Used inside iframe srcdoc where the parent's
// CSS variables are inaccessible. Keep in sync with :root { --bg-base } in index.css.
const BG_BASE_DARK = '#0B0F19'

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

interface SandboxEnv {
  id: string
  name: string
  base_url: string
  bearer_token: string
  description: string
  created_at?: string
  updated_at?: string
}

type SaveState = 'idle' | 'saving' | 'saved' | 'error'

// ---------------------------------------------------------------------------
// Local-storage fallback (offline / no server)
// ---------------------------------------------------------------------------

function loadLocalEnvs(): SandboxEnv[] {
  try {
    const raw = localStorage.getItem(LOCAL_STORAGE_KEY)
    if (raw) return JSON.parse(raw) as SandboxEnv[]
  } catch {}
  return []
}

function saveLocalEnvs(envs: SandboxEnv[]) {
  localStorage.setItem(LOCAL_STORAGE_KEY, JSON.stringify(envs))
}

// ---------------------------------------------------------------------------
// Scalar iframe builder
// ---------------------------------------------------------------------------

function buildScalarHtml(specUrl: string, env?: SandboxEnv | null) {
  const config: Record<string, unknown> = {
    theme: 'saturn',
    darkMode: true,
    hideClientButton: false,
    showSidebar: true,
  }
  if (env?.base_url) config.servers = [{ url: env.base_url, description: env.name }]
  if (env?.bearer_token) config.authentication = { http: { bearer: { token: env.bearer_token } } }

  return `<!doctype html>
<html>
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1" />
  <title>API Playground</title>
  <style>* { box-sizing: border-box; margin: 0; padding: 0; } body { background: ${BG_BASE_DARK}; }</style>
</head>
<body>
  <script id="api-reference" data-url="${specUrl}" data-configuration='${JSON.stringify(config)}'></script>
  <script src="https://cdn.jsdelivr.net/npm/@scalar/api-reference"></script>
</body>
</html>`
}

// ---------------------------------------------------------------------------
// Shared input styles
// ---------------------------------------------------------------------------

const INPUT_STYLE = {
  background: 'var(--bg-raised)',
  border: '1px solid var(--border)',
  color: 'var(--text-1)',
  fontFamily: 'var(--font-mono)',
} as const

function focusInput(e: React.FocusEvent<HTMLInputElement | HTMLTextAreaElement>) {
  e.currentTarget.style.borderColor = 'var(--cobalt)'
  e.currentTarget.style.boxShadow = 'var(--cobalt-focus-ring, 0 0 0 3px rgba(56,5,227,0.15))'
}
function blurInput(e: React.FocusEvent<HTMLInputElement | HTMLTextAreaElement>) {
  e.currentTarget.style.borderColor = 'var(--border)'
  e.currentTarget.style.boxShadow = ''
}

// ---------------------------------------------------------------------------
// Main page
// ---------------------------------------------------------------------------

export default function PlaygroundPage() {
  // Spec bar
  const [inputUrl, setInputUrl] = useState(DEFAULT_SPEC)
  const [activeUrl, setActiveUrl] = useState(DEFAULT_SPEC)

  // Stored specs (DB)
  const [storedSpecs, setStoredSpecs] = useState<Array<{
    id: string; service_name: string; git_ref: string; spec_format: string; captured_at: string
  }>>([])
  const [specsOpen, setSpecsOpen] = useState(false)

  // Sandbox environments
  const [envs, setEnvs] = useState<SandboxEnv[]>([])
  const [activeEnvId, setActiveEnvId] = useState<string | null>(null)
  const [serverMode, setServerMode] = useState(true)   // false = localStorage fallback
  const [envsLoading, setEnvsLoading] = useState(true)
  const [envOpen, setEnvOpen] = useState(false)

  // Env form
  const [editing, setEditing] = useState<SandboxEnv | null>(null)  // null = new
  const [formOpen, setFormOpen] = useState(false)
  const [form, setForm] = useState({ name: '', base_url: '', bearer_token: '', description: '' })
  const [saveState, setSaveState] = useState<SaveState>('idle')
  const [deleteConfirm, setDeleteConfirm] = useState<string | null>(null)

  const authHeader = { Authorization: `Bearer ${localStorage.getItem('radarToken') ?? ''}` }

  // ── Load stored specs ────────────────────────────────────────────────────
  useEffect(() => {
    fetch(`${API}/spec-versions`, { headers: authHeader })
      .then((r) => (r.ok ? r.json() : []))
      .then(setStoredSpecs)
      .catch(() => {})
  }, [])

  // ── Load sandbox environments ────────────────────────────────────────────
  useEffect(() => {
    fetch(`${API}/sandbox-envs`, { headers: authHeader })
      .then((r) => {
        if (!r.ok) throw new Error('server error')
        return r.json()
      })
      .then((data: SandboxEnv[]) => {
        setEnvs(data)
        setServerMode(true)
      })
      .catch(() => {
        setEnvs(loadLocalEnvs())
        setServerMode(false)
      })
      .finally(() => setEnvsLoading(false))
  }, [])

  const activeEnv = envs.find((e) => e.id === activeEnvId) ?? null

  // ── CRUD helpers ─────────────────────────────────────────────────────────

  function openNewForm() {
    setEditing(null)
    setForm({ name: '', base_url: '', bearer_token: '', description: '' })
    setSaveState('idle')
    setFormOpen(true)
  }

  function openEditForm(env: SandboxEnv) {
    setEditing(env)
    // Leave bearer_token blank: the server returns only a masked hint.
    // An empty field means "keep existing token"; typing a new value replaces it.
    setForm({ name: env.name, base_url: env.base_url, bearer_token: '', description: env.description })
    setSaveState('idle')
    setFormOpen(true)
  }

  async function handleSave() {
    if (!form.name.trim()) return
    setSaveState('saving')

    const payload = {
      name: form.name.trim(),
      base_url: form.base_url.trim(),
      bearer_token: form.bearer_token,
      description: form.description.trim(),
    }

    if (serverMode) {
      try {
        let resp: Response
        if (editing) {
          resp = await fetch(`${API}/sandbox-envs/${editing.id}`, {
            method: 'PUT',
            headers: { ...authHeader, 'Content-Type': 'application/json' },
            body: JSON.stringify(payload),
          })
        } else {
          resp = await fetch(`${API}/sandbox-envs`, {
            method: 'POST',
            headers: { ...authHeader, 'Content-Type': 'application/json' },
            body: JSON.stringify(payload),
          })
        }
        if (!resp.ok) throw new Error('server error')
        const saved: SandboxEnv = await resp.json()
        setEnvs((prev) =>
          editing
            ? prev.map((e) => (e.id === editing.id ? saved : e))
            : [...prev, saved],
        )
        if (!editing) setActiveEnvId(saved.id)
        setSaveState('saved')
        setTimeout(() => { setSaveState('idle'); setFormOpen(false) }, 800)
      } catch {
        setSaveState('error')
      }
    } else {
      // localStorage fallback
      // When editing, preserve the existing bearer_token if the field was left blank
      // (the edit form intentionally clears it so the masked hint isn't overwritten).
      const mergedPayload = editing && !payload.bearer_token
        ? { ...payload, bearer_token: editing.bearer_token }
        : payload
      const saved: SandboxEnv = editing
        ? { ...editing, ...mergedPayload, updated_at: new Date().toISOString() }
        : { id: crypto.randomUUID(), ...mergedPayload, created_at: new Date().toISOString(), updated_at: new Date().toISOString() }
      const next = editing ? envs.map((e) => (e.id === editing.id ? saved : e)) : [...envs, saved]
      setEnvs(next)
      saveLocalEnvs(next)
      if (!editing) setActiveEnvId(saved.id)
      setSaveState('saved')
      setTimeout(() => { setSaveState('idle'); setFormOpen(false) }, 800)
    }
  }

  async function handleDelete(id: string) {
    if (serverMode) {
      try {
        await fetch(`${API}/sandbox-envs/${id}`, { method: 'DELETE', headers: authHeader })
      } catch {}
    }
    const next = envs.filter((e) => e.id !== id)
    setEnvs(next)
    if (!serverMode) saveLocalEnvs(next)
    if (activeEnvId === id) setActiveEnvId(null)
    setDeleteConfirm(null)
    setFormOpen(false)
  }

  // ── Spec bar ─────────────────────────────────────────────────────────────

  const handleLoad = useCallback(() => {
    const trimmed = inputUrl.trim()
    if (trimmed) setActiveUrl(trimmed)
  }, [inputUrl])

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent<HTMLInputElement>) => { if (e.key === 'Enter') handleLoad() },
    [handleLoad],
  )

  function loadStoredSpec(id: string) {
    const url = `${API}/spec-versions/${id}/raw`
    setInputUrl(url)
    setActiveUrl(url)
    setSpecsOpen(false)
  }

  const iframeKey = `${activeUrl}::${activeEnvId ?? 'none'}`

  // Mode toggle: 'explorer' | 'csv'
  const [mode, setMode] = useState<'explorer' | 'csv'>('explorer')

  // ── Render ────────────────────────────────────────────────────────────────

  return (
    <div className="flex flex-col" style={{ height: '100vh' }}>
      <PageHeader
        tag="Playground"
        title="API Explorer"
        description="Interactive API playground powered by Scalar. Environments are shared across your team — no Postman required."
      />

      {/* ── URL bar ──────────────────────────────────────────────────────── */}
      <div
        className="flex items-center gap-3 px-14 py-4 flex-shrink-0"
        style={{ background: 'var(--bg-surface)', borderBottom: '1px solid var(--border)' }}
      >
        <Telescope className="h-4 w-4 flex-shrink-0" style={{ color: 'var(--text-3)' }} />
        <input
          type="url"
          value={inputUrl}
          onChange={(e) => setInputUrl(e.target.value)}
          onKeyDown={handleKeyDown}
          placeholder="https://api.example.com/openapi.yaml"
          className="flex-1 rounded-md border px-3 py-[7px] text-[12.5px] outline-none transition-colors"
          style={INPUT_STYLE}
          onFocus={focusInput}
          onBlur={blurInput}
        />
        <button
          onClick={handleLoad}
          className="flex-shrink-0 rounded-md px-4 py-[7px] text-[12.5px] font-semibold transition-all"
          style={{ background: 'var(--cobalt)', color: 'var(--text-inverse)' }}
          onMouseEnter={(e) => { e.currentTarget.style.background = 'var(--cobalt-mid)'; e.currentTarget.style.transform = 'translateY(-1px)' }}
          onMouseLeave={(e) => { e.currentTarget.style.background = 'var(--cobalt)'; e.currentTarget.style.transform = '' }}
        >
          Load Spec
        </button>
      </div>

      {/* ── Stored specs bar ─────────────────────────────────────────────── */}
      {storedSpecs.length > 0 && (
        <div className="flex-shrink-0" style={{ background: 'var(--bg-raised)', borderBottom: '1px solid var(--border)' }}>
          <div className="flex items-center gap-3 px-14 py-2.5">
            <button
              onClick={() => setSpecsOpen((o) => !o)}
              className="flex items-center gap-1.5 text-[11.5px] font-semibold uppercase tracking-[0.8px] transition-colors hover:text-[var(--text-2)]"
              style={{ color: 'var(--text-3)', fontFamily: 'var(--font-mono)' }}
            >
              {specsOpen ? <ChevronUp className="h-3 w-3" /> : <ChevronDown className="h-3 w-3" />}
              <Database className="h-3 w-3" />
              Stored Specs
            </button>
            <span
              className="rounded-full px-2 py-0.5 text-[10px] font-medium"
              style={{ background: 'var(--bg-active)', color: 'var(--cobalt-mid)', fontFamily: 'var(--font-mono)' }}
            >
              {storedSpecs.length}
            </span>
          </div>
          {specsOpen && (
            <div className="px-14 pb-4 pt-1 flex flex-wrap gap-2">
              {storedSpecs.map((spec) => (
                <button
                  key={spec.id}
                  onClick={() => loadStoredSpec(spec.id)}
                  className="flex flex-col items-start rounded-md px-3 py-2 text-left transition-all hover:border-[var(--cobalt-mid)]"
                  style={{ background: 'var(--bg-surface)', border: '1px solid var(--border)', minWidth: '180px', maxWidth: '240px' }}
                >
                  <span className="text-[12px] font-semibold truncate w-full" style={{ color: 'var(--text-1)' }}>
                    {spec.service_name}
                  </span>
                  <span className="text-[10.5px] mt-0.5 truncate w-full" style={{ color: 'var(--text-3)', fontFamily: 'var(--font-mono)' }}>
                    {spec.git_ref}
                  </span>
                  <div className="flex items-center gap-2 mt-1.5">
                    <span className="rounded px-1.5 py-0.5 text-[9.5px] uppercase tracking-wide font-semibold" style={{ background: 'var(--bg-hover)', color: 'var(--text-dim)' }}>
                      {spec.spec_format}
                    </span>
                    <span className="text-[10px]" style={{ color: 'var(--text-dim)' }}>
                      {new Date(spec.captured_at).toLocaleDateString()}
                    </span>
                  </div>
                </button>
              ))}
            </div>
          )}
        </div>
      )}

      {/* ── Environments bar ──────────────────────────────────────────────── */}
      <div className="flex-shrink-0" style={{ background: 'var(--bg-raised)', borderBottom: '1px solid var(--border)' }}>

        {/* Toggle row */}
        <div className="flex items-center gap-3 px-14 py-2.5">
          <button
            onClick={() => setEnvOpen((o) => !o)}
            className="flex items-center gap-1.5 text-[11.5px] font-semibold uppercase tracking-[0.8px] transition-colors hover:text-[var(--text-2)]"
            style={{ color: 'var(--text-3)', fontFamily: 'var(--font-mono)' }}
          >
            {envOpen ? <ChevronUp className="h-3 w-3" /> : <ChevronDown className="h-3 w-3" />}
            Environments
          </button>

          {/* Active env chip */}
          {!envOpen && activeEnv && (
            <button
              onClick={() => { setActiveEnvId(null) }}
              className="flex items-center gap-1.5 rounded px-2 py-0.5 text-[11px] font-medium group"
              style={{ background: 'var(--cobalt)', color: 'var(--text-inverse)', fontFamily: 'var(--font-mono)' }}
              title="Click to deactivate"
            >
              {activeEnv.name}
              <X className="h-2.5 w-2.5 opacity-0 group-hover:opacity-100 transition-opacity" />
            </button>
          )}
          {!envOpen && !activeEnv && !envsLoading && (
            <span className="text-[11px]" style={{ color: 'var(--text-dim)', fontFamily: 'var(--font-mono)' }}>
              none active
            </span>
          )}
          {envsLoading && <Loader2 className="h-3 w-3 animate-spin" style={{ color: 'var(--text-dim)' }} />}

          {/* Server/local indicator */}
          <div className="ml-auto flex items-center gap-1.5">
            {!envsLoading && (
              <>
                {serverMode
                  ? <Cloud className="h-3 w-3" style={{ color: 'var(--teal)' }} />
                  : <HardDrive className="h-3 w-3" style={{ color: 'var(--amber)' }} />}
                <span className="text-[10px]" style={{ color: serverMode ? 'var(--teal)' : 'var(--amber)', fontFamily: 'var(--font-mono)' }}>
                  {serverMode ? 'shared' : 'local only'}
                </span>
              </>
            )}
          </div>
        </div>

        {/* Expanded panel */}
        {envOpen && (
          <div className="px-14 pb-5 pt-2 space-y-4">

            {/* Env chips + New button */}
            <div className="flex flex-wrap gap-2 items-center">
              {envs.map((env) => {
                const isActive = env.id === activeEnvId
                return (
                  <button
                    key={env.id}
                    onClick={() => {
                      setActiveEnvId(isActive ? null : env.id)
                      openEditForm(env)
                    }}
                    className="rounded-md px-3 py-1.5 text-[11.5px] font-medium transition-all"
                    style={{
                      background: isActive ? 'var(--cobalt)' : 'var(--bg-surface)',
                      border: `1px solid ${isActive ? 'var(--cobalt)' : 'var(--border)'}`,
                      color: isActive ? '#fff' : 'var(--text-2)',
                      fontFamily: 'var(--font-mono)',
                    }}
                  >
                    {env.name}
                    {env.description && (
                      <span className="ml-1.5 opacity-60 font-normal text-[10.5px]">{env.description}</span>
                    )}
                  </button>
                )
              })}
              <button
                onClick={openNewForm}
                className="flex items-center gap-1.5 rounded-md px-3 py-1.5 text-[11.5px] font-medium transition-colors hover:border-[var(--text-3)]"
                style={{ background: 'transparent', border: '1px dashed var(--border)', color: 'var(--text-3)' }}
              >
                <Plus className="h-3 w-3" />
                New environment
              </button>
            </div>

            {/* Form (new or edit) */}
            {formOpen && (
              <EnvForm
                form={form}
                setForm={setForm}
                editing={editing}
                saveState={saveState}
                deleteConfirm={deleteConfirm}
                setDeleteConfirm={setDeleteConfirm}
                onSave={handleSave}
                onDelete={handleDelete}
                onActivate={(id) => setActiveEnvId(activeEnvId === id ? null : id)}
                activeEnvId={activeEnvId}
                onClose={() => setFormOpen(false)}
              />
            )}

            {!serverMode && (
              <p className="text-[11px]" style={{ color: 'var(--amber)', fontFamily: 'var(--font-mono)' }}>
                ⚠ Server unreachable — environments are saved in this browser only and not shared with teammates.
              </p>
            )}
          </div>
        )}
      </div>

      {/* ── Mode tabs ─────────────────────────────────────────────────────── */}
      <div className="flex items-center gap-0 px-14 pt-3 flex-shrink-0" style={{ borderBottom: '1px solid var(--border)' }}>
        <button
          onClick={() => setMode('explorer')}
          className="flex items-center gap-1.5 px-4 py-2 text-[12px] font-medium border-b-2 transition-colors"
          style={{
            color: mode === 'explorer' ? 'var(--cobalt-mid)' : 'var(--text-3)',
            borderColor: mode === 'explorer' ? 'var(--cobalt)' : 'transparent',
            marginBottom: '-1px',
          }}
        >
          <Telescope className="h-3.5 w-3.5" />
          API Explorer
        </button>
        <button
          onClick={() => setMode('csv')}
          className="flex items-center gap-1.5 px-4 py-2 text-[12px] font-medium border-b-2 transition-colors"
          style={{
            color: mode === 'csv' ? 'var(--cobalt-mid)' : 'var(--text-3)',
            borderColor: mode === 'csv' ? 'var(--cobalt)' : 'transparent',
            marginBottom: '-1px',
          }}
        >
          <Rows className="h-3.5 w-3.5" />
          CSV Data Runner
        </button>
      </div>

      {/* ── Scalar iframe ─────────────────────────────────────────────────── */}
      {mode === 'explorer' && (
        <div className="flex-1 overflow-hidden">
          <iframe
            key={iframeKey}
            srcDoc={buildScalarHtml(activeUrl, activeEnv)}
            title="API Playground"
            sandbox="allow-scripts allow-same-origin allow-forms allow-popups"
            style={{ width: '100%', height: '100%', border: 'none' }}
          />
        </div>
      )}

      {/* ── CSV Runner ────────────────────────────────────────────────────── */}
      {mode === 'csv' && (
        <div className="flex-1 overflow-y-auto">
          <CsvRunnerPanel />
        </div>
      )}
    </div>
  )
}

// ---------------------------------------------------------------------------
// Env form sub-component
// ---------------------------------------------------------------------------

interface EnvFormProps {
  form: { name: string; base_url: string; bearer_token: string; description: string }
  setForm: React.Dispatch<React.SetStateAction<{ name: string; base_url: string; bearer_token: string; description: string }>>
  editing: SandboxEnv | null
  saveState: SaveState
  deleteConfirm: string | null
  setDeleteConfirm: (id: string | null) => void
  onSave: () => void
  onDelete: (id: string) => void
  onActivate: (id: string) => void
  activeEnvId: string | null
  onClose: () => void
}

function EnvForm({ form, setForm, editing, saveState, deleteConfirm, setDeleteConfirm, onSave, onDelete, onActivate, activeEnvId, onClose }: EnvFormProps) {
  const nameRef = useRef<HTMLInputElement>(null)
  useEffect(() => { nameRef.current?.focus() }, [])

  const isActive = editing ? editing.id === activeEnvId : false

  return (
    <div
      className="rounded-lg p-4 space-y-3"
      style={{ background: 'var(--bg-surface)', border: '1px solid var(--border)' }}
    >
      <div className="flex items-center justify-between">
        <p className="text-[11px] font-bold uppercase tracking-widest" style={{ color: 'var(--text-dim)' }}>
          {editing ? 'Edit environment' : 'New environment'}
        </p>
        <button onClick={onClose} style={{ color: 'var(--text-3)' }}>
          <X className="h-3.5 w-3.5" />
        </button>
      </div>

      <div className="grid gap-2" style={{ gridTemplateColumns: '1fr 1fr' }}>
        <div className="space-y-1">
          <label className="text-[10.5px] font-semibold uppercase tracking-wide" style={{ color: 'var(--text-dim)' }}>Name *</label>
          <input
            ref={nameRef}
            type="text"
            placeholder="Production sandbox"
            value={form.name}
            onChange={(e) => setForm((f) => ({ ...f, name: e.target.value }))}
            className="w-full rounded-md border px-3 py-[6px] text-[12px] outline-none transition-colors"
            style={INPUT_STYLE}
            onFocus={focusInput}
            onBlur={blurInput}
          />
        </div>
        <div className="space-y-1">
          <label className="text-[10.5px] font-semibold uppercase tracking-wide" style={{ color: 'var(--text-dim)' }}>Short description</label>
          <input
            type="text"
            placeholder="e.g. Base demo tenant"
            value={form.description}
            onChange={(e) => setForm((f) => ({ ...f, description: e.target.value }))}
            className="w-full rounded-md border px-3 py-[6px] text-[12px] outline-none transition-colors"
            style={INPUT_STYLE}
            onFocus={focusInput}
            onBlur={blurInput}
          />
        </div>
        <div className="space-y-1">
          <label className="text-[10.5px] font-semibold uppercase tracking-wide" style={{ color: 'var(--text-dim)' }}>Base URL</label>
          <input
            type="url"
            placeholder="https://sandbox.base.com/api"
            value={form.base_url}
            onChange={(e) => setForm((f) => ({ ...f, base_url: e.target.value }))}
            className="w-full rounded-md border px-3 py-[6px] text-[12px] outline-none transition-colors"
            style={INPUT_STYLE}
            onFocus={focusInput}
            onBlur={blurInput}
          />
        </div>
        <div className="space-y-1">
          <label className="text-[10.5px] font-semibold uppercase tracking-wide" style={{ color: 'var(--text-dim)' }}>Bearer token</label>
          <input
            type="password"
            placeholder={editing ? 'Leave blank to keep existing token' : 'Demo API key'}
            value={form.bearer_token}
            onChange={(e) => setForm((f) => ({ ...f, bearer_token: e.target.value }))}
            className="w-full rounded-md border px-3 py-[6px] text-[12px] outline-none transition-colors"
            style={INPUT_STYLE}
            onFocus={focusInput}
            onBlur={blurInput}
          />
        </div>
      </div>

      <div className="flex items-center gap-2 pt-1">
        {/* Save */}
        <button
          onClick={onSave}
          disabled={saveState === 'saving' || !form.name.trim()}
          className="flex items-center gap-1.5 rounded-md px-4 py-[6px] text-[12px] font-semibold transition-all disabled:opacity-40"
          style={{ background: saveState === 'saved' ? 'var(--teal)' : 'var(--cobalt)', color: 'var(--text-inverse)' }}
        >
          {saveState === 'saving' && <Loader2 className="h-3 w-3 animate-spin" />}
          {saveState === 'saved' && <Check className="h-3 w-3" />}
          {saveState === 'error' && <X className="h-3 w-3" />}
          {saveState === 'idle' && (editing ? 'Save' : 'Add')}
          {saveState === 'saving' && 'Saving…'}
          {saveState === 'saved' && 'Saved'}
          {saveState === 'error' && 'Error — retry'}
        </button>

        {/* Activate / deactivate */}
        {editing && (
          <button
            onClick={() => onActivate(editing.id)}
            className="rounded-md px-3 py-[6px] text-[12px] font-medium transition-colors"
            style={{
              background: isActive ? 'var(--bg-active)' : 'var(--bg-hover)',
              border: `1px solid ${isActive ? 'var(--cobalt)' : 'var(--border)'}`,
              color: isActive ? 'var(--cobalt-mid)' : 'var(--text-2)',
            }}
          >
            {isActive ? 'Active — click to deactivate' : 'Set as active'}
          </button>
        )}

        {/* Delete */}
        {editing && (
          deleteConfirm === editing.id ? (
            <div className="flex items-center gap-1.5 ml-auto">
              <span className="text-[11.5px]" style={{ color: 'var(--text-2)' }}>Delete this environment?</span>
              <button
                onClick={() => onDelete(editing.id)}
                className="rounded-md px-3 py-[5px] text-[12px] font-semibold"
                style={{ background: 'var(--red, #ef4444)', color: '#fff' }}
              >
                Delete
              </button>
              <button
                onClick={() => setDeleteConfirm(null)}
                className="rounded-md px-3 py-[5px] text-[12px]"
                style={{ color: 'var(--text-3)' }}
              >
                Cancel
              </button>
            </div>
          ) : (
            <button
              onClick={() => setDeleteConfirm(editing.id)}
              className="ml-auto flex items-center gap-1.5 rounded-md px-3 py-[6px] text-[12px] font-medium transition-colors"
              style={{ border: '1px solid var(--border)', color: 'var(--red, #ef4444)' }}
            >
              <Trash2 className="h-3 w-3" />
              Delete
            </button>
          )
        )}
      </div>
    </div>
  )
}

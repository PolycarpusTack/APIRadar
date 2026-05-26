import { useEffect, useState } from 'react'
import { Plus, Trash2, X, Info } from 'lucide-react'
import { useNavigate } from 'react-router-dom'
import PageHeader from '../components/PageHeader'
import Badge from '../components/Badge'
import EmptyState from '../components/EmptyState'
import TermTooltip from '../components/TermTooltip'
import { api, ApiError } from '../lib/apiClient'

const CALLOUT_DISMISSED_KEY = 'radar_evolution_rules_callout_dismissed'

function PlatformEngineerCallout() {
  const navigate = useNavigate()
  const [dismissed, setDismissed] = useState(
    () => localStorage.getItem(CALLOUT_DISMISSED_KEY) === '1'
  )

  if (dismissed) return null

  function dismiss() {
    localStorage.setItem(CALLOUT_DISMISSED_KEY, '1')
    setDismissed(true)
  }

  return (
    <div
      className="flex items-start gap-3 rounded-lg p-4 text-[12.5px]"
      style={{ background: 'var(--blue-bg, rgba(56,120,227,0.08))', border: '1px solid var(--blue-dim, rgba(56,120,227,0.25))', color: 'var(--text-2)' }}
    >
      <Info className="h-4 w-4 mt-0.5 flex-shrink-0" style={{ color: 'var(--blue, #3878e3)' }} />
      <p className="flex-1 leading-relaxed">
        <span className="font-semibold" style={{ color: 'var(--text-1)' }}>Evolution rules are for platform engineers.</span>
        {' '}They let you relax the default severity of specific change kinds across your organisation.
        If you're not sure whether you need this, you probably don't.{' '}
        <button
          onClick={() => navigate('/help')}
          className="underline decoration-dotted hover:no-underline"
          style={{ color: 'var(--blue, #3878e3)' }}
        >
          Learn more
        </button>
      </p>
      <button onClick={dismiss} className="flex-shrink-0" style={{ color: 'var(--text-3)' }}>
        <X className="h-4 w-4" />
      </button>
    </div>
  )
}

interface EvolutionRule {
  id: string
  name: string
  change_kind: string
  path_pattern: string | null
  severity_override: string
  enabled: boolean
  created_at: string
}

const CHANGE_KINDS = [
  'field_removed', 'field_added', 'type_changed', 'required_changed',
  'operation_removed', 'operation_added', 'parameter_removed', 'response_removed',
  'enum_value_removed', 'enum_value_added', 'nullability_changed',
  'request_body_added', 'request_body_removed',
]

interface FormState {
  name: string
  change_kind: string
  path_pattern: string
  severity_override: string
}

const DEFAULT_FORM: FormState = {
  name: '',
  change_kind: 'field_removed',
  path_pattern: '',
  severity_override: 'non_breaking_risky',
}

function severityVariant(s: string): 'warn' | 'ok' | 'neutral' {
  if (s === 'safe') return 'ok'
  if (s === 'non_breaking_risky') return 'warn'
  return 'neutral'
}

export default function EvolutionRulesPage() {
  const [rules, setRules] = useState<EvolutionRule[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [showCreate, setShowCreate] = useState(false)
  const [form, setForm] = useState<FormState>(DEFAULT_FORM)
  const [creating, setCreating] = useState(false)
  const [createError, setCreateError] = useState<string | null>(null)
  const [toggling, setToggling] = useState<Record<string, boolean>>({})
  const [deleting, setDeleting] = useState<Record<string, boolean>>({})

  function loadRules() {
    setLoading(true)
    api.get<{ entries: EvolutionRule[] }>('/v1/evolution-rules')
      .then((data) => setRules(data.entries ?? []))
      .catch((e) => setError(e instanceof ApiError ? e.message : String(e)))
      .finally(() => setLoading(false))
  }

  useEffect(() => { loadRules() }, [])

  function handleCreate(e: React.FormEvent) {
    e.preventDefault()
    setCreating(true)
    setCreateError(null)
    api.post('/v1/evolution-rules', {
      name: form.name,
      change_kind: form.change_kind,
      path_pattern: form.path_pattern || undefined,
      severity_override: form.severity_override,
    })
      .then(() => {
        setShowCreate(false)
        setForm(DEFAULT_FORM)
        loadRules()
      })
      .catch((e) => setCreateError(e instanceof ApiError ? e.message : String(e)))
      .finally(() => setCreating(false))
  }

  function handleToggle(rule: EvolutionRule) {
    setToggling((t) => ({ ...t, [rule.id]: true }))
    api.patch(`/v1/evolution-rules/${rule.id}`, { enabled: !rule.enabled })
      .then(() => loadRules())
      .catch(() => {})
      .finally(() => setToggling((t) => ({ ...t, [rule.id]: false })))
  }

  function handleDelete(id: string) {
    if (!confirm('Delete this evolution rule?')) return
    setDeleting((d) => ({ ...d, [id]: true }))
    api.del(`/v1/evolution-rules/${id}`)
      .then(() => loadRules())
      .catch(() => {})
      .finally(() => setDeleting((d) => ({ ...d, [id]: false })))
  }

  return (
    <div>
      <PageHeader
        tag="Governance"
        title="Evolution Rules"
        description="Override the default severity of specific change kinds. Rules only downgrade severity — they cannot make safe changes breaking. First matching rule per change wins."
      />

      <div className="px-14 py-8 space-y-6">
        {/* J-7: audience callout — dismissible, stored in localStorage */}
        <PlatformEngineerCallout />

        {/* How it works callout */}
        <div
          className="rounded-lg p-4 text-[12.5px]"
          style={{ background: 'var(--bg-raised)', border: '1px solid var(--border)', color: 'var(--text-2)' }}
        >
          <p className="font-semibold mb-1" style={{ color: 'var(--text-1)' }}>How evolution rules work</p>
          <p>
            When a diff is fetched, the server evaluates active rules against each change.
            If a rule matches the <code style={{ fontFamily: 'var(--font-mono)' }}>change_kind</code> and optional
            path pattern, and the override is a downgrade (e.g. breaking → non_breaking_risky), the change's severity
            is relaxed and an <code style={{ fontFamily: 'var(--font-mono)' }}>applied_rule</code> field is included in the response.
            Policy decisions use the overridden severity.
          </p>
        </div>

        {/* Action bar */}
        <div className="flex justify-end">
          <button
            onClick={() => setShowCreate((v) => !v)}
            className="flex items-center gap-1.5 rounded-md px-3 py-1.5 text-[12px] font-medium"
            style={{ background: 'var(--cobalt)', color: 'var(--text-inverse)' }}
          >
            <Plus className="h-3.5 w-3.5" />
            Add rule
          </button>
        </div>

        {/* Create form */}
        {showCreate && (
          <div className="rounded-lg p-5" style={{ border: '1px solid var(--border-mid)', background: 'var(--bg-surface)' }}>
            <div className="flex items-center justify-between mb-4">
              <p className="text-[13px] font-semibold" style={{ color: 'var(--text-1)' }}>New Evolution Rule</p>
              <button onClick={() => { setShowCreate(false); setCreateError(null) }}>
                <X className="h-4 w-4" style={{ color: 'var(--text-3)' }} />
              </button>
            </div>
            <form onSubmit={handleCreate} className="grid grid-cols-2 gap-3">
              <div className="col-span-2">
                <label className="block mb-1 text-[10.5px] font-semibold uppercase tracking-[0.8px]" style={{ color: 'var(--text-3)' }}>
                  Name
                </label>
                <input
                  required
                  value={form.name}
                  onChange={(e) => setForm((f) => ({ ...f, name: e.target.value }))}
                  placeholder="Allow adding optional headers"
                  className="w-full rounded-md border px-2.5 py-1.5 text-[12.5px]"
                  style={{ borderColor: 'var(--border-mid)', background: 'var(--bg-raised)', color: 'var(--text-1)' }}
                />
              </div>
              <div>
                <label className="flex items-center gap-1 mb-1 text-[10.5px] font-semibold uppercase tracking-[0.8px]" style={{ color: 'var(--text-3)' }}>
                  Change kind
                  <TermTooltip term={`change_kind_${form.change_kind}` as `change_kind_${string}`} placement="bottom" />
                </label>
                <select
                  value={form.change_kind}
                  onChange={(e) => setForm((f) => ({ ...f, change_kind: e.target.value }))}
                  className="w-full rounded-md border px-2.5 py-1.5 text-[12.5px]"
                  style={{ borderColor: 'var(--border-mid)', background: 'var(--bg-raised)', color: 'var(--text-1)' }}
                >
                  {CHANGE_KINDS.map((k) => (
                    <option key={k} value={k}>{k}</option>
                  ))}
                </select>
              </div>
              <div>
                <label className="block mb-1 text-[10.5px] font-semibold uppercase tracking-[0.8px]" style={{ color: 'var(--text-3)' }}>
                  Severity override
                </label>
                <select
                  value={form.severity_override}
                  onChange={(e) => setForm((f) => ({ ...f, severity_override: e.target.value }))}
                  className="w-full rounded-md border px-2.5 py-1.5 text-[12.5px]"
                  style={{ borderColor: 'var(--border-mid)', background: 'var(--bg-raised)', color: 'var(--text-1)' }}
                >
                  <option value="non_breaking_risky">non_breaking_risky</option>
                  <option value="safe">safe</option>
                </select>
              </div>
              <div className="col-span-2">
                <label className="block mb-1 text-[10.5px] font-semibold uppercase tracking-[0.8px]" style={{ color: 'var(--text-3)' }}>
                  Path pattern (optional — glob, e.g. <code style={{ fontFamily: 'var(--font-mono)' }}>users.*</code> or <code style={{ fontFamily: 'var(--font-mono)' }}>**.legacy_id</code>)
                </label>
                <input
                  value={form.path_pattern}
                  onChange={(e) => setForm((f) => ({ ...f, path_pattern: e.target.value }))}
                  placeholder="Leave blank to match any field path"
                  className="w-full rounded-md border px-2.5 py-1.5 text-[12.5px]"
                  style={{ borderColor: 'var(--border-mid)', background: 'var(--bg-raised)', color: 'var(--text-1)', fontFamily: 'var(--font-mono)' }}
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
                  {creating ? 'Creating…' : 'Create rule'}
                </button>
              </div>
            </form>
          </div>
        )}

        {/* Rules table */}
        <div className="overflow-hidden rounded-lg" style={{ border: '1px solid var(--border)', background: 'var(--bg-surface)' }}>
          <div className="flex items-center px-4 py-3" style={{ borderBottom: '1px solid var(--border)' }}>
            <p className="text-[11px] font-semibold uppercase tracking-[0.8px]" style={{ color: 'var(--text-3)' }}>
              {loading ? 'Loading…' : `${rules.length} rule${rules.length !== 1 ? 's' : ''}`}
            </p>
          </div>

          {error ? (
            <div className="px-4 py-3 text-[12.5px]" style={{ color: 'var(--red)' }}>
              Failed to load evolution rules: {error}
            </div>
          ) : rules.length === 0 && !loading ? (
            <EmptyState
              icon={Plus}
              title="No evolution rules defined"
              description="Add a rule to relax the default severity of specific change kinds. For example, treat adding an enum value as safe rather than non-breaking-risky."
            />
          ) : (
            <table className="w-full border-collapse">
              <thead>
                <tr>
                  {['Name', 'Change Kind', 'Path Pattern', 'Override', 'Status', ''].map((col) => (
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
                {rules.map((rule) => (
                  <tr
                    key={rule.id}
                    className="group"
                    style={{ borderBottom: '1px solid var(--border)', opacity: rule.enabled ? 1 : 0.5 }}
                  >
                    <td
                      className="px-3 py-2.5 font-medium group-hover:bg-[var(--bg-hover)]"
                      style={{ fontSize: '12.5px', color: 'var(--text-1)' }}
                    >
                      {rule.name}
                    </td>
                    <td
                      className="px-3 py-2.5 group-hover:bg-[var(--bg-hover)]"
                      style={{ fontFamily: 'var(--font-mono)', fontSize: '11.5px', color: 'var(--teal)' }}
                    >
                      {rule.change_kind}
                    </td>
                    <td
                      className="px-3 py-2.5 group-hover:bg-[var(--bg-hover)]"
                      style={{ fontFamily: 'var(--font-mono)', fontSize: '11.5px', color: 'var(--text-3)' }}
                    >
                      {rule.path_pattern ?? <span style={{ color: 'var(--text-dim)' }}>any</span>}
                    </td>
                    <td className="px-3 py-2.5 group-hover:bg-[var(--bg-hover)]">
                      <Badge variant={severityVariant(rule.severity_override)}>
                        {rule.severity_override.replace(/_/g, ' ')}
                      </Badge>
                    </td>
                    <td className="px-3 py-2.5 group-hover:bg-[var(--bg-hover)]">
                      <button
                        onClick={() => handleToggle(rule)}
                        disabled={toggling[rule.id]}
                        className="rounded-md border px-2.5 py-0.5 text-[11px] font-medium transition-colors hover:bg-[var(--bg-hover)]"
                        style={{
                          borderColor: 'var(--border-mid)',
                          color: rule.enabled ? 'var(--green)' : 'var(--text-3)',
                          opacity: toggling[rule.id] ? 0.5 : 1,
                        }}
                      >
                        {rule.enabled ? 'enabled' : 'disabled'}
                      </button>
                    </td>
                    <td className="px-3 py-2.5 group-hover:bg-[var(--bg-hover)]">
                      <button
                        onClick={() => handleDelete(rule.id)}
                        disabled={deleting[rule.id]}
                        className="rounded p-1 transition-colors hover:bg-[var(--bg-hover)]"
                        title="Delete rule"
                        style={{ opacity: deleting[rule.id] ? 0.4 : 1 }}
                      >
                        <Trash2 className="h-3.5 w-3.5" style={{ color: 'var(--red)' }} />
                      </button>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </div>
      </div>
    </div>
  )
}

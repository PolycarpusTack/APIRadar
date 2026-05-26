import { useEffect, useState } from 'react'
import { useParams, useNavigate } from 'react-router-dom'
import { ArrowLeft, ExternalLink, CheckCircle, Plus, X, Sparkles } from 'lucide-react'
import Badge from '../components/Badge'
import TermTooltip from '../components/TermTooltip'
import { api } from '../lib/apiClient'

interface DiffChange {
  path: string
  kind: string
  severity: string
  description: string | null
}

interface DiffDetail {
  id: string
  from_git_ref: string
  to_git_ref: string
  pr_url: string | null
  created_at: string
  changes: DiffChange[]
}

interface BlastEntry {
  consumer: {
    id: string
    name: string
    repo_url: string
    owner_team: string
    contact: string
  }
  confidence: string
  last_seen: string
  has_runtime_usage: boolean
  has_call_site: boolean
}

interface BlastRadius {
  diff_id: string
  service_id: string
  lookback_days: number
  entries: BlastEntry[]
}

interface Acknowledgement {
  id: string
  diff_id: string | null
  service_id: string | null
  consumer_id: string | null
  acknowledged_by: string
  reason: string | null
  expires_at: string | null
  created_at: string
}

interface AckFormState {
  acknowledged_by: string
  reason: string
  expires_at: string
}

function severityVariant(s: string): 'err' | 'warn' | 'ok' | 'neutral' {
  if (s === 'breaking') return 'err'
  if (s === 'non_breaking_risky') return 'warn'
  if (s === 'safe') return 'ok'
  return 'neutral'
}

function confidenceVariant(c: string): 'err' | 'warn' | 'neutral' {
  if (c === 'high') return 'err'
  if (c === 'medium') return 'warn'
  return 'neutral'
}

function kindLabel(k: string) {
  return k.replace(/_/g, ' ')
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

function TableHeader({ cols }: { cols: string[] }) {
  return (
    <thead>
      <tr>
        {cols.map((col) => (
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
  )
}

const DEFAULT_ACK_FORM: AckFormState = { acknowledged_by: '', reason: '', expires_at: '' }

export default function DiffDetailPage() {
  const { id } = useParams<{ id: string }>()
  const navigate = useNavigate()

  const [diff, setDiff] = useState<DiffDetail | null>(null)
  const [blast, setBlast] = useState<BlastRadius | null>(null)
  const [acks, setAcks] = useState<Acknowledgement[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [showAckForm, setShowAckForm] = useState(false)
  const [ackForm, setAckForm] = useState<AckFormState>(DEFAULT_ACK_FORM)
  const [submittingAck, setSubmittingAck] = useState(false)
  const [ackError, setAckError] = useState<string | null>(null)
  const [generatingNote, setGeneratingNote] = useState(false)
  const [generatedNote, setGeneratedNote] = useState<string | null>(null)
  const [noteError, setNoteError] = useState<string | null>(null)

  function loadAcks() {
    if (!id) return
    api.get<{ entries: Acknowledgement[] }>(`/v1/diffs/${id}/acknowledgements`)
      .then((data) => setAcks(data.entries ?? []))
      .catch(() => {})
  }

  useEffect(() => {
    if (!id) return
    setLoading(true)

    Promise.all([
      api.get<DiffDetail>(`/v1/diffs/${id}`),
      api.get<BlastRadius>(`/v1/diffs/${id}/blast-radius`),
    ])
      .then(([d, b]) => { setDiff(d); setBlast(b) })
      .catch((e: Error) => setError(e.message))
      .finally(() => setLoading(false))

    loadAcks()
  }, [id])

  function handleAckSubmit(e: React.FormEvent) {
    e.preventDefault()
    if (!id) return
    setSubmittingAck(true)
    setAckError(null)
    api.post('/v1/acknowledgements', {
      diff_id: id,
      acknowledged_by: ackForm.acknowledged_by,
      reason: ackForm.reason || undefined,
      expires_at: ackForm.expires_at || undefined,
    })
      .then(() => {
        setShowAckForm(false)
        setAckForm(DEFAULT_ACK_FORM)
        loadAcks()
      })
      .catch((e: Error) => setAckError(e.message))
      .finally(() => setSubmittingAck(false))
  }

  async function generateReleaseNote() {
    if (!id) return
    setGeneratingNote(true)
    setNoteError(null)
    try {
      const data = await api.post<{ content: string }>(`/v1/diffs/${id}/release-notes/generate`)
      setGeneratedNote(data.content)
    } catch (e) {
      setNoteError((e as Error).message)
    } finally {
      setGeneratingNote(false)
    }
  }

  if (loading) {
    return (
      <div className="flex items-center justify-center h-64">
        <p className="text-[12.5px]" style={{ color: 'var(--text-3)' }}>Loading…</p>
      </div>
    )
  }

  if (error || !diff) {
    return (
      <div className="px-14 py-10">
        <p className="text-[12.5px]" style={{ color: 'var(--red)' }}>
          {error ?? 'Diff not found'}
        </p>
      </div>
    )
  }

  const breakingCount = diff.changes.filter((c) => c.severity === 'breaking').length
  const riskyCount = diff.changes.filter((c) => c.severity === 'non_breaking_risky').length
  const safeCount = diff.changes.filter((c) => c.severity === 'safe').length

  return (
    <div>
      {/* Back bar */}
      <div
        className="flex items-center gap-3 border-b px-14 py-4"
        style={{ background: 'var(--bg-surface)', borderColor: 'var(--border)' }}
      >
        <button
          onClick={() => navigate('/diffs')}
          className="flex items-center gap-1.5 text-[12px] transition-colors hover:text-[var(--text-1)]"
          style={{ color: 'var(--text-3)', fontFamily: 'var(--font-mono)' }}
        >
          <ArrowLeft className="h-3.5 w-3.5" />
          All Diffs
        </button>
        <span style={{ color: 'var(--border-hi)' }}>/</span>
        <span className="text-[12px] truncate max-w-xs" style={{ color: 'var(--text-2)', fontFamily: 'var(--font-mono)' }}>
          {diff.id.slice(0, 8)}…
        </span>
      </div>

      {/* Header card */}
      <div
        className="border-b px-14 py-8"
        style={{ background: 'var(--bg-surface)', borderColor: 'var(--border)' }}
      >
        <div className="flex items-start justify-between gap-6 mb-5">
          <div>
            <p className="mb-2 text-[10.5px] font-medium uppercase tracking-[1.5px]" style={{ fontFamily: 'var(--font-mono)', color: 'var(--cobalt-mid)' }}>
              Schema Diff
            </p>
            <p className="mb-1 text-[22px] font-bold tracking-[-0.8px]" style={{ fontFamily: 'var(--font-head)', color: 'var(--text-1)' }}>
              <span style={{ color: 'var(--text-2)' }}>{diff.from_git_ref}</span>
              {' → '}
              <span style={{ color: 'var(--cobalt-mid)' }}>{diff.to_git_ref}</span>
            </p>
            <p className="text-[12.5px]" style={{ fontFamily: 'var(--font-mono)', color: 'var(--text-3)' }}>
              {formatDate(diff.created_at)}
            </p>
          </div>
          <div className="flex items-center gap-2">
            {diff.pr_url && (
              <a
                href={diff.pr_url}
                target="_blank"
                rel="noreferrer"
                className="flex items-center gap-1.5 rounded-md border px-3 py-1.5 text-[12px] font-medium transition-colors hover:bg-[var(--bg-hover)]"
                style={{ borderColor: 'var(--border-mid)', color: 'var(--text-2)' }}
              >
                <ExternalLink className="h-3.5 w-3.5" />
                Pull Request
              </a>
            )}
            {diff.changes.length > 0 && (
              <button
                onClick={generateReleaseNote}
                disabled={generatingNote}
                className="flex items-center gap-1.5 rounded-md border px-3 py-1.5 text-[12px] font-medium transition-colors hover:bg-[var(--bg-hover)] disabled:opacity-50"
                style={{ borderColor: 'rgba(56,5,227,0.3)', color: 'var(--cobalt-mid)' }}
              >
                <Sparkles className="h-3.5 w-3.5" />
                {generatingNote ? 'Generating…' : 'Generate Release Notes'}
              </button>
            )}
          </div>
        </div>
        <div className="flex gap-3">
          {breakingCount > 0 && <Badge variant="err">{breakingCount} breaking</Badge>}
          {riskyCount > 0 && <Badge variant="warn">{riskyCount} risky</Badge>}
          {safeCount > 0 && <Badge variant="ok">{safeCount} safe</Badge>}
          {diff.changes.length === 0 && <Badge variant="neutral">No changes</Badge>}
        </div>
      </div>

      <div className="px-14 py-8 space-y-8">
        {/* Changes table */}
        <section>
          <p className="mb-3 text-[9.5px] font-semibold uppercase tracking-[1.2px]" style={{ color: 'var(--text-dim)' }}>
            Changes ({diff.changes.length})
          </p>
          <div className="overflow-hidden rounded-lg" style={{ border: '1px solid var(--border)', background: 'var(--bg-surface)' }}>
            {diff.changes.length === 0 ? (
              <p className="px-4 py-6 text-center text-[12.5px]" style={{ color: 'var(--text-3)' }}>No changes recorded for this diff.</p>
            ) : (
              <table className="w-full border-collapse">
                <TableHeader cols={['Severity', 'Field Path', 'Kind', 'Description']} />
                <tbody>
                  {diff.changes.map((c, i) => (
                    <tr key={i} className="group" style={{ borderBottom: '1px solid var(--border)' }}>
                      <td className="px-3 py-2.5 group-hover:bg-[var(--bg-hover)]">
                        <Badge variant={severityVariant(c.severity)}>{c.severity.replace(/_/g, ' ')}</Badge>
                      </td>
                      <td
                        className="px-3 py-2.5 group-hover:bg-[var(--bg-hover)]"
                        style={{ fontFamily: 'var(--font-mono)', fontSize: '11.5px', color: 'var(--text-1)' }}
                      >
                        {c.path}
                      </td>
                      <td
                        className="px-3 py-2.5 group-hover:bg-[var(--bg-hover)]"
                        style={{ fontSize: '12px', color: 'var(--text-2)' }}
                      >
                        {kindLabel(c.kind)}
                      </td>
                      <td
                        className="px-3 py-2.5 group-hover:bg-[var(--bg-hover)]"
                        style={{ fontSize: '12px', color: 'var(--text-3)' }}
                      >
                        {c.description ?? <span style={{ color: 'var(--text-dim)' }}>—</span>}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            )}
          </div>
        </section>

        {/* Blast Radius table */}
        <section>
          <p className="mb-3 flex items-center gap-1.5 text-[9.5px] font-semibold uppercase tracking-[1.2px]" style={{ color: 'var(--text-dim)' }}>
            Blast Radius — consumers at risk
            <TermTooltip term="blast_radius" placement="top" />
          </p>
          <div className="overflow-hidden rounded-lg" style={{ border: '1px solid var(--border)', background: 'var(--bg-surface)' }}>
            {!blast || blast.entries.length === 0 ? (
              <p className="px-4 py-6 text-center text-[12.5px]" style={{ color: 'var(--text-3)' }}>
                No consumers affected — either no consumers are subscribed or none have used the changed fields within the {blast?.lookback_days ?? 30}-day lookback window.
              </p>
            ) : (
              <table className="w-full border-collapse">
                <TableHeader cols={['Consumer', 'Confidence', 'Last Seen', 'Team', 'Contact', 'Evidence']} />
                <tbody>
                  {blast.entries.map((e, i) => (
                    <tr key={i} className="group" style={{ borderBottom: '1px solid var(--border)' }}>
                      <td
                        className="px-3 py-2.5 font-medium group-hover:bg-[var(--bg-hover)]"
                        style={{ fontSize: '12.5px', color: 'var(--text-1)' }}
                      >
                        {e.consumer.name}
                      </td>
                      <td className="px-3 py-2.5 group-hover:bg-[var(--bg-hover)]">
                        <span className="inline-flex items-center gap-1">
                          <Badge variant={confidenceVariant(e.confidence)}>{e.confidence}</Badge>
                          <TermTooltip
                            term={`confidence_${e.confidence}` as 'confidence_high' | 'confidence_medium' | 'confidence_low'}
                            placement="top"
                          />
                        </span>
                      </td>
                      <td
                        className="px-3 py-2.5 group-hover:bg-[var(--bg-hover)]"
                        style={{ fontFamily: 'var(--font-mono)', fontSize: '11.5px', color: 'var(--text-3)' }}
                      >
                        {e.last_seen ? formatDate(e.last_seen) : '—'}
                      </td>
                      <td className="px-3 py-2.5 group-hover:bg-[var(--bg-hover)]" style={{ fontSize: '12px', color: 'var(--text-2)' }}>
                        {e.consumer.owner_team || '—'}
                      </td>
                      <td className="px-3 py-2.5 group-hover:bg-[var(--bg-hover)]" style={{ fontFamily: 'var(--font-mono)', fontSize: '11.5px', color: 'var(--text-2)' }}>
                        {e.consumer.contact || '—'}
                      </td>
                      <td className="px-3 py-2.5 group-hover:bg-[var(--bg-hover)]">
                        <div className="flex gap-1.5">
                          {e.has_runtime_usage && <Badge variant="cobalt">usage</Badge>}
                          {e.has_call_site && <Badge variant="neon">call site</Badge>}
                        </div>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            )}
          </div>
        </section>

        {/* Acknowledgements */}
        <section>
          <div className="flex items-center justify-between mb-3">
            <p className="text-[9.5px] font-semibold uppercase tracking-[1.2px]" style={{ color: 'var(--text-dim)' }}>
              Acknowledgements ({acks.length})
            </p>
            <button
              onClick={() => setShowAckForm((v) => !v)}
              className="flex items-center gap-1 rounded-md border px-2.5 py-1 text-[11.5px] font-medium transition-colors hover:bg-[var(--bg-hover)]"
              style={{ borderColor: 'var(--border-mid)', color: 'var(--text-2)' }}
            >
              <Plus className="h-3 w-3" />
              Acknowledge
            </button>
          </div>

          {showAckForm && (
            <div className="mb-3 rounded-lg p-4" style={{ border: '1px solid rgba(56,5,227,0.3)', background: 'var(--bg-surface)' }}>
              <div className="flex items-center justify-between mb-3">
                <p className="text-[12px] font-semibold" style={{ color: 'var(--text-1)' }}>Create Acknowledgement</p>
                <button onClick={() => { setShowAckForm(false); setAckError(null) }}>
                  <X className="h-4 w-4" style={{ color: 'var(--text-3)' }} />
                </button>
              </div>
              <form onSubmit={handleAckSubmit} className="space-y-3">
                <div>
                  <label className="block mb-1 text-[10.5px] font-semibold uppercase tracking-[0.8px]" style={{ color: 'var(--text-3)' }}>
                    Acknowledged by
                  </label>
                  <input
                    required
                    value={ackForm.acknowledged_by}
                    onChange={(e) => setAckForm((f) => ({ ...f, acknowledged_by: e.target.value }))}
                    placeholder="alice@example.com"
                    className="w-full rounded-md border px-2.5 py-1.5 text-[12.5px]"
                    style={{ borderColor: 'var(--border-mid)', background: 'var(--bg-raised)', color: 'var(--text-1)' }}
                  />
                </div>
                <div>
                  <label className="block mb-1 text-[10.5px] font-semibold uppercase tracking-[0.8px]" style={{ color: 'var(--text-3)' }}>
                    Reason
                  </label>
                  <input
                    value={ackForm.reason}
                    onChange={(e) => setAckForm((f) => ({ ...f, reason: e.target.value }))}
                    placeholder="All consumers have migrated to v2"
                    className="w-full rounded-md border px-2.5 py-1.5 text-[12.5px]"
                    style={{ borderColor: 'var(--border-mid)', background: 'var(--bg-raised)', color: 'var(--text-1)' }}
                  />
                </div>
                <div>
                  <label className="block mb-1 text-[10.5px] font-semibold uppercase tracking-[0.8px]" style={{ color: 'var(--text-3)' }}>
                    Expires at (optional ISO 8601)
                  </label>
                  <input
                    value={ackForm.expires_at}
                    onChange={(e) => setAckForm((f) => ({ ...f, expires_at: e.target.value }))}
                    placeholder="2026-12-31T00:00:00Z"
                    className="w-full rounded-md border px-2.5 py-1.5 text-[12.5px]"
                    style={{ borderColor: 'var(--border-mid)', background: 'var(--bg-raised)', color: 'var(--text-1)', fontFamily: 'var(--font-mono)' }}
                  />
                </div>
                {ackError && (
                  <p className="text-[12px]" style={{ color: 'var(--red)' }}>{ackError}</p>
                )}
                <div className="flex justify-end gap-2">
                  <button
                    type="button"
                    onClick={() => { setShowAckForm(false); setAckError(null) }}
                    className="rounded-md px-3 py-1.5 text-[12px]"
                    style={{ border: '1px solid var(--border-mid)', color: 'var(--text-2)', background: 'var(--bg-raised)' }}
                  >
                    Cancel
                  </button>
                  <button
                    type="submit"
                    disabled={submittingAck}
                    className="rounded-md px-3 py-1.5 text-[12px] font-medium"
                    style={{ background: 'var(--cobalt)', color: 'var(--text-inverse)', opacity: submittingAck ? 0.6 : 1 }}
                  >
                    {submittingAck ? 'Saving…' : 'Create acknowledgement'}
                  </button>
                </div>
              </form>
            </div>
          )}

          <div className="overflow-hidden rounded-lg" style={{ border: '1px solid var(--border)', background: 'var(--bg-surface)' }}>
            {acks.length === 0 ? (
              <p className="px-4 py-6 text-center text-[12.5px]" style={{ color: 'var(--text-3)' }}>
                No acknowledgements yet. Create one to formally accept this breaking change and allow CI to proceed.
              </p>
            ) : (
              <table className="w-full border-collapse">
                <TableHeader cols={['Acknowledged By', 'Reason', 'Expires', 'Date']} />
                <tbody>
                  {acks.map((a) => (
                    <tr key={a.id} className="group" style={{ borderBottom: '1px solid var(--border)' }}>
                      <td
                        className="px-3 py-2.5 font-medium group-hover:bg-[var(--bg-hover)]"
                        style={{ fontSize: '12.5px', color: 'var(--text-1)' }}
                      >
                        <div className="flex items-center gap-1.5">
                          <CheckCircle className="h-3.5 w-3.5 flex-shrink-0" style={{ color: 'var(--green)' }} />
                          {a.acknowledged_by}
                        </div>
                      </td>
                      <td
                        className="px-3 py-2.5 group-hover:bg-[var(--bg-hover)]"
                        style={{ fontSize: '12px', color: 'var(--text-2)', maxWidth: '260px' }}
                      >
                        <span className="truncate block" title={a.reason ?? ''}>
                          {a.reason ?? <span style={{ color: 'var(--text-dim)' }}>—</span>}
                        </span>
                      </td>
                      <td
                        className="px-3 py-2.5 group-hover:bg-[var(--bg-hover)]"
                        style={{ fontFamily: 'var(--font-mono)', fontSize: '11.5px', color: 'var(--text-3)' }}
                      >
                        {a.expires_at ? formatDate(a.expires_at) : <span style={{ color: 'var(--text-dim)' }}>never</span>}
                      </td>
                      <td
                        className="px-3 py-2.5 group-hover:bg-[var(--bg-hover)]"
                        style={{ fontFamily: 'var(--font-mono)', fontSize: '11.5px', color: 'var(--text-3)' }}
                      >
                        {formatDate(a.created_at)}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            )}
          </div>
        </section>

        {/* Generated release note */}
        {(generatedNote || noteError) && (
          <section>
            <p className="mb-3 text-[9.5px] font-semibold uppercase tracking-[1.2px]" style={{ color: 'var(--text-dim)' }}>
              Generated Release Notes
            </p>
            {noteError && (
              <p className="text-[12.5px]" style={{ color: 'var(--red)' }}>{noteError}</p>
            )}
            {generatedNote && (
              <div className="overflow-hidden rounded-lg" style={{ border: '1px solid var(--border)', background: 'var(--bg-surface)' }}>
                <div className="flex items-center justify-between px-4 py-2.5" style={{ borderBottom: '1px solid var(--border)' }}>
                  <p className="text-[11px]" style={{ color: 'var(--text-3)', fontFamily: 'var(--font-mono)' }}>Markdown — saved as draft in Release Notes</p>
                  <button
                    onClick={() => navigate('/release-notes')}
                    className="text-[11.5px] font-medium transition-opacity hover:opacity-70"
                    style={{ color: 'var(--cobalt-mid)' }}
                  >
                    View all release notes →
                  </button>
                </div>
                <pre
                  className="overflow-x-auto p-4 text-[12px] leading-relaxed whitespace-pre-wrap"
                  style={{
                    fontFamily: 'var(--font-mono)',
                    color: 'var(--text-2)',
                    maxHeight: '480px',
                    overflowY: 'auto',
                  }}
                >
                  {generatedNote}
                </pre>
              </div>
            )}
          </section>
        )}
      </div>
    </div>
  )
}

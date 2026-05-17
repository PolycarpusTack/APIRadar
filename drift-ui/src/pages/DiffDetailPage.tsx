import { useEffect, useState } from 'react'
import { useParams, useNavigate } from 'react-router-dom'
import { ArrowLeft, ExternalLink } from 'lucide-react'
import Badge from '../components/Badge'

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

export default function DiffDetailPage() {
  const { id } = useParams<{ id: string }>()
  const navigate = useNavigate()

  const [diff, setDiff] = useState<DiffDetail | null>(null)
  const [blast, setBlast] = useState<BlastRadius | null>(null)
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    if (!id) return
    setLoading(true)

    Promise.all([
      fetch(`/v1/diffs/${id}`).then((r) => {
        if (!r.ok) throw new Error(`HTTP ${r.status}`)
        return r.json() as Promise<DiffDetail>
      }),
      fetch(`/v1/diffs/${id}/blast-radius`).then((r) => {
        if (!r.ok) throw new Error(`HTTP ${r.status}`)
        return r.json() as Promise<BlastRadius>
      }),
    ])
      .then(([d, b]) => { setDiff(d); setBlast(b) })
      .catch((e: Error) => setError(e.message))
      .finally(() => setLoading(false))
  }, [id])

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
          <p className="mb-3 text-[9.5px] font-semibold uppercase tracking-[1.2px]" style={{ color: 'var(--text-dim)' }}>
            Blast Radius — consumers at risk
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
                        <Badge variant={confidenceVariant(e.confidence)}>{e.confidence}</Badge>
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
      </div>
    </div>
  )
}

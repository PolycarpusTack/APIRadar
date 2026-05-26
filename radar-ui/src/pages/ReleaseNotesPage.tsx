import { useEffect, useState } from 'react'
import { FileText, ChevronDown, ChevronRight, ArrowRight } from 'lucide-react'
import PageHeader from '../components/PageHeader'
import EmptyState from '../components/EmptyState'
import { api, ApiError } from '../lib/apiClient'

interface NoteRow {
  id: string
  diff_id: string
  from_git_ref: string
  to_git_ref: string
  status: string
  created_at: string
}

interface NoteDetail extends NoteRow {
  content: string
}

const STATUS_COLOURS: Record<string, { bg: string; text: string }> = {
  draft:       { bg: '#f3f4f6', text: '#6b7280' },
  reviewed:    { bg: '#eff6ff', text: '#2563eb' },
  published:   { bg: '#f0fdf4', text: '#16a34a' },
  superseded:  { bg: '#fafafa', text: '#9ca3af' },
}

// State-machine: which transitions are allowed from each status?
const NEXT_STATUSES: Record<string, string[]> = {
  draft:      ['reviewed', 'superseded'],
  reviewed:   ['published', 'draft', 'superseded'],
  published:  ['superseded'],
  superseded: [],
}

function StatusBadge({ status }: { status: string }) {
  const col = STATUS_COLOURS[status] ?? STATUS_COLOURS.draft
  return (
    <span
      className="rounded-full px-2 py-0.5 text-[10px] font-semibold uppercase tracking-[0.8px]"
      style={{ background: col.bg, color: col.text }}
    >
      {status}
    </span>
  )
}

function NoteCard({ row, onStatusChange }: { row: NoteRow; onStatusChange: (id: string, status: string) => void }) {
  const [open, setOpen] = useState(false)
  const [detail, setDetail] = useState<NoteDetail | null>(null)
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [transitioning, setTransitioning] = useState(false)

  async function toggle() {
    if (open) { setOpen(false); return }
    setOpen(true)
    if (detail) return
    setLoading(true); setError(null)
    try {
      setDetail(await api.get<NoteDetail>(`/v1/release-notes/${row.id}`))
    } catch (e) {
      setError(e instanceof ApiError ? e.message : String(e))
    } finally {
      setLoading(false)
    }
  }

  async function transition(newStatus: string) {
    setTransitioning(true)
    try {
      await api.patch(`/v1/release-notes/${row.id}/status`, { status: newStatus })
      onStatusChange(row.id, newStatus)
    } catch (e) {
      setError(e instanceof ApiError ? e.message : String(e))
    } finally {
      setTransitioning(false)
    }
  }

  const date = row.created_at.slice(0, 10)
  const nextStatuses = NEXT_STATUSES[row.status] ?? []

  return (
    <div style={{ borderBottom: '1px solid var(--border)' }}>
      <button
        onClick={toggle}
        className="flex w-full items-center gap-3 px-4 py-3 text-left transition-colors hover:bg-[var(--bg-hover)]"
      >
        {open
          ? <ChevronDown  className="h-3.5 w-3.5 flex-shrink-0" style={{ color: 'var(--text-dim)' }} />
          : <ChevronRight className="h-3.5 w-3.5 flex-shrink-0" style={{ color: 'var(--text-dim)' }} />
        }
        <span className="flex-1 text-[12.5px] font-medium" style={{ color: 'var(--text-1)', fontFamily: 'var(--font-mono)' }}>
          {row.from_git_ref.slice(0, 8)} → {row.to_git_ref.slice(0, 8)}
        </span>
        <StatusBadge status={row.status} />
        <span className="ml-3 text-[11px]" style={{ color: 'var(--text-3)', fontFamily: 'var(--font-mono)' }}>
          {row.diff_id.slice(0, 8)}
        </span>
        <span className="ml-4 text-[11px]" style={{ color: 'var(--text-dim)' }}>{date}</span>
      </button>

      {open && (
        <div className="px-4 pb-5 pt-1">
          {/* Status transition controls */}
          {nextStatuses.length > 0 && (
            <div className="mb-3 flex items-center gap-2 flex-wrap">
              <span className="text-[11px]" style={{ color: 'var(--text-3)' }}>Move to:</span>
              {nextStatuses.map((ns) => (
                <button
                  key={ns}
                  disabled={transitioning}
                  onClick={() => transition(ns)}
                  className="inline-flex items-center gap-1 rounded-full px-3 py-1 text-[11px] font-semibold transition-opacity disabled:opacity-50"
                  style={{
                    background: STATUS_COLOURS[ns]?.bg ?? '#f3f4f6',
                    color: STATUS_COLOURS[ns]?.text ?? '#6b7280',
                    border: '1px solid currentColor',
                    cursor: 'pointer',
                  }}
                >
                  <ArrowRight className="h-2.5 w-2.5" />
                  {ns}
                </button>
              ))}
            </div>
          )}

          {error && <p className="mb-2 text-[12px]" style={{ color: 'var(--red)' }}>{error}</p>}
          {loading && <p className="text-[12px]" style={{ color: 'var(--text-3)' }}>Loading…</p>}
          {detail && (
            <pre
              className="overflow-x-auto rounded-md p-4 text-[12px] leading-relaxed whitespace-pre-wrap"
              style={{
                background: 'var(--bg-raised)',
                border: '1px solid var(--border)',
                color: 'var(--text-2)',
                fontFamily: 'var(--font-mono)',
              }}
            >
              {detail.content}
            </pre>
          )}
        </div>
      )}
    </div>
  )
}

export default function ReleaseNotesPage() {
  const [rows, setRows] = useState<NoteRow[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    api.get<NoteRow[]>('/v1/release-notes')
      .then(setRows)
      .catch((e) => setError(e instanceof ApiError ? e.message : String(e)))
      .finally(() => setLoading(false))
  }, [])

  function handleStatusChange(id: string, newStatus: string) {
    setRows(prev => prev.map(r => r.id === id ? { ...r, status: newStatus } : r))
  }

  return (
    <div>
      <PageHeader
        tag="Docs"
        title="Release Notes"
        description="AI-generated release notes for each schema diff — breaking change summaries, migration checklists, and per-consumer impact."
      />

      <div className="px-14 py-8">
        <div
          className="overflow-hidden rounded-lg"
          style={{ border: '1px solid var(--border)', background: 'var(--bg-surface)' }}
        >
          <div className="flex items-center justify-between px-4 py-3" style={{ borderBottom: '1px solid var(--border)' }}>
            <p className="text-[11px] font-semibold uppercase tracking-[0.8px]" style={{ color: 'var(--text-3)' }}>
              {loading ? 'Loading…' : `${rows.length} note${rows.length !== 1 ? 's' : ''}`}
            </p>
            <p className="text-[11px]" style={{ color: 'var(--text-dim)', fontFamily: 'var(--font-mono)' }}>
              radar explain --diff-id &lt;id&gt; --release-notes --api-url &lt;url&gt;
            </p>
          </div>

          {error ? (
            <div className="px-4 py-3 text-[12.5px]" style={{ color: 'var(--red)' }}>
              Failed to load: {error}
            </div>
          ) : !loading && rows.length === 0 ? (
            <EmptyState
              icon={FileText}
              title="No release notes yet"
              description="Run radar explain --diff-id <id> --release-notes --api-url <url> to generate and store notes."
            />
          ) : (
            rows.map(row => (
              <NoteCard key={row.id} row={row} onStatusChange={handleStatusChange} />
            ))
          )}
        </div>
      </div>
    </div>
  )
}

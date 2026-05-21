import { useEffect, useState } from 'react'
import { FileText, ChevronDown, ChevronRight } from 'lucide-react'
import PageHeader from '../components/PageHeader'
import EmptyState from '../components/EmptyState'

interface NoteRow {
  id: string
  diff_id: string
  from_git_ref: string
  to_git_ref: string
  created_at: string
}

interface NoteDetail extends NoteRow {
  content: string
}

function NoteCard({ row }: { row: NoteRow }) {
  const [open, setOpen] = useState(false)
  const [detail, setDetail] = useState<NoteDetail | null>(null)
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)

  async function toggle() {
    if (open) { setOpen(false); return }
    setOpen(true)
    if (detail) return
    setLoading(true); setError(null)
    try {
      const resp = await fetch(`/v1/release-notes/${row.id}`)
      if (!resp.ok) throw new Error(`HTTP ${resp.status}`)
      setDetail(await resp.json() as NoteDetail)
    } catch (e) {
      setError((e as Error).message)
    } finally {
      setLoading(false)
    }
  }

  const date = row.created_at.slice(0, 10)

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
        <span className="text-[11px]" style={{ color: 'var(--text-3)', fontFamily: 'var(--font-mono)' }}>
          {row.diff_id.slice(0, 8)}
        </span>
        <span className="ml-4 text-[11px]" style={{ color: 'var(--text-dim)' }}>{date}</span>
      </button>

      {open && (
        <div className="px-4 pb-5 pt-1">
          {loading && <p className="text-[12px]" style={{ color: 'var(--text-3)' }}>Loading…</p>}
          {error   && <p className="text-[12px]" style={{ color: 'var(--red)' }}>{error}</p>}
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
    fetch('/v1/release-notes')
      .then(r => { if (!r.ok) throw new Error(`HTTP ${r.status}`); return r.json() as Promise<NoteRow[]> })
      .then(setRows)
      .catch((e: Error) => setError(e.message))
      .finally(() => setLoading(false))
  }, [])

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
            rows.map(row => <NoteCard key={row.id} row={row} />)
          )}
        </div>
      </div>
    </div>
  )
}

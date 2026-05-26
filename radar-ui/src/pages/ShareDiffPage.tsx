import { useEffect, useState } from 'react'
import { useParams } from 'react-router-dom'
import { ExternalLink, AlertCircle, GitCompare } from 'lucide-react'
import Badge from '../components/Badge'

interface DiffChange {
  path: string
  kind: string
  severity: string
  description: string | null
}

interface SharedDiff {
  id: string
  service_name: string
  from_git_ref: string
  to_git_ref: string
  pr_url: string | null
  created_at: string
  changes: DiffChange[]
}

function SeverityBadge({ severity }: { severity: string }) {
  const variants: Record<string, 'err' | 'warn' | 'info'> = {
    breaking: 'err',
    non_breaking_risky: 'warn',
    safe: 'info',
  }
  return <Badge variant={variants[severity] ?? 'info'}>{severity.replace(/_/g, ' ')}</Badge>
}

export default function ShareDiffPage() {
  const { token } = useParams<{ token: string }>()
  const [diff, setDiff] = useState<SharedDiff | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [loading, setLoading] = useState(true)

  useEffect(() => {
    if (!token) return
    fetch(`/share/${token}`)
      .then(async r => {
        if (!r.ok) throw new Error(r.status === 404 ? 'This shared diff does not exist or has been revoked.' : `HTTP ${r.status}`)
        return r.json() as Promise<SharedDiff>
      })
      .then(setDiff)
      .catch((e: unknown) => setError(String(e)))
      .finally(() => setLoading(false))
  }, [token])

  const breakingCount = diff?.changes.filter(c => c.severity === 'breaking').length ?? 0

  return (
    <div className="min-h-screen" style={{ background: 'var(--bg-base)', color: 'var(--text-1)' }}>
      {/* Header bar */}
      <header
        className="flex items-center justify-between px-8 py-4 border-b"
        style={{ background: 'var(--bg-surface)', borderColor: 'var(--border)' }}
      >
        <div className="flex items-center gap-3">
          <GitCompare className="h-5 w-5" style={{ color: 'var(--cobalt-mid)' }} />
          <span style={{ fontFamily: 'var(--font-head)', fontSize: '17px', fontWeight: 700 }}>API Radar</span>
          <span className="text-[12px]" style={{ color: 'var(--text-dim)' }}>shared diff view</span>
        </div>
        <a
          href="/"
          className="flex items-center gap-1.5 rounded-md px-3 py-1.5 text-[12.5px] font-semibold transition-opacity hover:opacity-80"
          style={{ background: 'var(--cobalt)', color: 'var(--text-inverse)' }}
        >
          <ExternalLink className="h-3.5 w-3.5" />
          Open in API Radar
        </a>
      </header>

      <main className="mx-auto max-w-4xl px-8 py-8">
        {loading && (
          <p className="text-[13px]" style={{ color: 'var(--text-3)' }}>Loading…</p>
        )}

        {error && (
          <div className="flex items-center gap-3 rounded-lg border px-5 py-4" style={{ border: '1px solid var(--red)', background: 'var(--bg-surface)' }}>
            <AlertCircle className="h-5 w-5 flex-shrink-0" style={{ color: 'var(--red)' }} />
            <p className="text-[13.5px]" style={{ color: 'var(--red)' }}>{error}</p>
          </div>
        )}

        {diff && (
          <>
            {/* Diff header */}
            <div className="mb-6 rounded-lg border px-6 py-5" style={{ border: '1px solid var(--border)', background: 'var(--bg-surface)' }}>
              <p className="text-[11px] font-semibold uppercase tracking-[0.8px] mb-1" style={{ color: 'var(--text-dim)' }}>Service</p>
              <p className="text-[18px] font-semibold mb-4" style={{ color: 'var(--text-1)', fontFamily: 'var(--font-head)' }}>{diff.service_name}</p>
              <div className="flex items-center gap-4 flex-wrap">
                <div>
                  <p className="text-[10.5px]" style={{ color: 'var(--text-dim)' }}>Base</p>
                  <code className="text-[12px]" style={{ color: 'var(--text-2)', fontFamily: 'var(--font-mono)' }}>{diff.from_git_ref}</code>
                </div>
                <span style={{ color: 'var(--text-dim)' }}>→</span>
                <div>
                  <p className="text-[10.5px]" style={{ color: 'var(--text-dim)' }}>Head</p>
                  <code className="text-[12px]" style={{ color: 'var(--text-2)', fontFamily: 'var(--font-mono)' }}>{diff.to_git_ref}</code>
                </div>
                <div className="ml-auto flex items-center gap-3">
                  {breakingCount > 0 && (
                    <span className="rounded-full px-3 py-1 text-[12px] font-semibold" style={{ background: 'color-mix(in srgb, var(--red) 12%, transparent)', color: 'var(--red)' }}>
                      {breakingCount} breaking
                    </span>
                  )}
                  <span className="text-[12px]" style={{ color: 'var(--text-dim)' }}>{diff.changes.length} changes total</span>
                </div>
              </div>
              {diff.pr_url && (
                <a href={diff.pr_url} target="_blank" rel="noreferrer" className="mt-3 flex items-center gap-1.5 text-[12px]" style={{ color: 'var(--cobalt-mid)' }}>
                  <ExternalLink className="h-3 w-3" />
                  View pull request
                </a>
              )}
            </div>

            {/* Changes table */}
            <div className="rounded-lg border overflow-hidden" style={{ border: '1px solid var(--border)', background: 'var(--bg-surface)' }}>
              <div className="px-5 py-3 border-b" style={{ borderColor: 'var(--border)' }}>
                <p className="text-[12.5px] font-semibold" style={{ color: 'var(--text-1)' }}>Changes</p>
              </div>
              {diff.changes.length === 0 ? (
                <p className="px-5 py-4 text-[12.5px]" style={{ color: 'var(--text-dim)' }}>No changes detected.</p>
              ) : (
                <table className="w-full text-[12px]">
                  <thead>
                    <tr style={{ borderBottom: '1px solid var(--border)' }}>
                      <th className="px-5 py-2.5 text-left font-medium" style={{ color: 'var(--text-dim)' }}>Path</th>
                      <th className="px-3 py-2.5 text-left font-medium" style={{ color: 'var(--text-dim)' }}>Kind</th>
                      <th className="px-3 py-2.5 text-left font-medium" style={{ color: 'var(--text-dim)' }}>Severity</th>
                      <th className="px-3 py-2.5 text-left font-medium" style={{ color: 'var(--text-dim)' }}>Description</th>
                    </tr>
                  </thead>
                  <tbody>
                    {diff.changes.map((c, i) => (
                      <tr key={i} style={{ borderBottom: i < diff.changes.length - 1 ? '1px solid var(--border)' : undefined }}>
                        <td className="px-5 py-2.5 font-mono" style={{ color: 'var(--text-2)' }}>{c.path}</td>
                        <td className="px-3 py-2.5" style={{ color: 'var(--text-3)' }}>{c.kind}</td>
                        <td className="px-3 py-2.5"><SeverityBadge severity={c.severity} /></td>
                        <td className="px-3 py-2.5" style={{ color: 'var(--text-3)' }}>{c.description ?? ''}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              )}
            </div>

            {/* Sign-in CTA */}
            <div className="mt-6 rounded-lg border px-6 py-4 flex items-center justify-between" style={{ border: '1px solid var(--border)', background: 'var(--bg-surface)' }}>
              <p className="text-[12.5px]" style={{ color: 'var(--text-2)' }}>
                Sign in to see blast radius, consumer evidence, and acknowledgements.
              </p>
              <a
                href="/"
                className="rounded-md px-4 py-2 text-[12.5px] font-semibold"
                style={{ background: 'var(--cobalt)', color: 'var(--text-inverse)' }}
              >
                Open full view
              </a>
            </div>
          </>
        )}
      </main>
    </div>
  )
}

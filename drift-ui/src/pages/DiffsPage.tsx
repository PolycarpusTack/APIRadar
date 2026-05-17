import { GitCompare } from 'lucide-react'
import PageHeader from '../components/PageHeader'
import Badge from '../components/Badge'
import EmptyState from '../components/EmptyState'

const TABLE_COLS = ['Date', 'Service', 'Refs', 'Breaking', 'Risky', 'Safe', 'Blast Radius']

type SeverityCount = { breaking: number; risky: number; safe: number }

interface DiffRow {
  id: string
  date: string
  service: string
  fromRef: string
  toRef: string
  counts: SeverityCount
  affectedConsumers: number
}

function DiffTable({ rows }: { rows: DiffRow[] }) {
  if (rows.length === 0) {
    return (
      <EmptyState
        icon={GitCompare}
        title="No diffs recorded yet"
        description="Run drift check --api-url … to post your first schema diff and see it here."
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
                style={{
                  background: 'var(--bg-raised)',
                  borderColor: 'var(--border)',
                  color: 'var(--text-3)',
                }}
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
            >
              <td
                className="px-3 py-2.5 text-[12.5px] group-hover:bg-[var(--bg-hover)]"
                style={{ fontFamily: 'var(--font-mono)', color: 'var(--text-3)', fontSize: '11.5px' }}
              >
                {row.date}
              </td>
              <td
                className="px-3 py-2.5 text-[12.5px] font-medium group-hover:bg-[var(--bg-hover)]"
                style={{ color: 'var(--text-1)' }}
              >
                {row.service}
              </td>
              <td
                className="px-3 py-2.5 group-hover:bg-[var(--bg-hover)]"
                style={{ fontFamily: 'var(--font-mono)', fontSize: '11px', color: 'var(--text-3)' }}
              >
                <span style={{ color: 'var(--text-2)' }}>{row.fromRef}</span>
                <span className="mx-1">→</span>
                <span style={{ color: 'var(--cobalt-mid)' }}>{row.toRef}</span>
              </td>
              <td className="px-3 py-2.5 group-hover:bg-[var(--bg-hover)]">
                {row.counts.breaking > 0 ? (
                  <Badge variant="err">{row.counts.breaking}</Badge>
                ) : (
                  <span style={{ color: 'var(--text-dim)', fontSize: '12px' }}>—</span>
                )}
              </td>
              <td className="px-3 py-2.5 group-hover:bg-[var(--bg-hover)]">
                {row.counts.risky > 0 ? (
                  <Badge variant="warn">{row.counts.risky}</Badge>
                ) : (
                  <span style={{ color: 'var(--text-dim)', fontSize: '12px' }}>—</span>
                )}
              </td>
              <td className="px-3 py-2.5 group-hover:bg-[var(--bg-hover)]">
                {row.counts.safe > 0 ? (
                  <Badge variant="ok">{row.counts.safe}</Badge>
                ) : (
                  <span style={{ color: 'var(--text-dim)', fontSize: '12px' }}>—</span>
                )}
              </td>
              <td className="px-3 py-2.5 group-hover:bg-[var(--bg-hover)]">
                {row.affectedConsumers > 0 ? (
                  <Badge variant="cobalt">{row.affectedConsumers} consumer{row.affectedConsumers !== 1 ? 's' : ''}</Badge>
                ) : (
                  <span style={{ color: 'var(--text-dim)', fontSize: '12px' }}>—</span>
                )}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  )
}

export default function DiffsPage() {
  const rows: DiffRow[] = []

  return (
    <div>
      <PageHeader
        tag="Monitor"
        title="Schema Diffs"
        description="Every drift check run that was posted to this server. Click a row to see the full blast-radius report and release notes."
      />

      <div className="px-14 py-8">
        <div
          className="rounded-lg overflow-hidden"
          style={{ border: '1px solid var(--border)', background: 'var(--bg-surface)' }}
        >
          <div
            className="flex items-center justify-between px-4 py-3"
            style={{ borderBottom: rows.length > 0 ? '1px solid var(--border)' : undefined }}
          >
            <p
              className="text-[11px] font-semibold uppercase tracking-[0.8px]"
              style={{ color: 'var(--text-3)' }}
            >
              All diffs
            </p>
          </div>
          <DiffTable rows={rows} />
        </div>
      </div>
    </div>
  )
}

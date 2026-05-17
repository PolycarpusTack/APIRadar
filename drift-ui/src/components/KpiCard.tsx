type KpiVariant = 'teal' | 'amber' | 'red' | 'cobalt' | 'blue'

interface KpiCardProps {
  label: string
  value: string | number
  meta?: string
  variant?: KpiVariant
}

const barColor: Record<KpiVariant, string> = {
  teal:   'var(--teal)',
  amber:  'var(--amber)',
  red:    'var(--red)',
  cobalt: 'var(--cobalt)',
  blue:   'var(--blue)',
}

export default function KpiCard({ label, value, meta, variant = 'cobalt' }: KpiCardProps) {
  return (
    <div
      className="relative overflow-hidden rounded-lg border p-4"
      style={{ background: 'var(--bg-surface)', borderColor: 'var(--border)' }}
    >
      <div
        className="absolute bottom-0 left-0 right-0 h-[2px]"
        style={{ background: barColor[variant] }}
      />
      <p
        className="text-[10px] font-semibold uppercase tracking-[0.8px]"
        style={{ color: 'var(--text-3)' }}
      >
        {label}
      </p>
      <p
        className="mt-1 mb-0.5 text-[28px] font-semibold leading-none tabular-nums"
        style={{ fontFamily: 'var(--font-mono)', color: 'var(--text-1)' }}
      >
        {value}
      </p>
      {meta && (
        <p className="text-[11px]" style={{ color: 'var(--text-3)' }}>
          {meta}
        </p>
      )}
    </div>
  )
}

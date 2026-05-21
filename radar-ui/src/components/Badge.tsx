type BadgeVariant = 'ok' | 'warn' | 'err' | 'info' | 'cobalt' | 'neon' | 'neutral' | 'purple'

interface BadgeProps {
  variant?: BadgeVariant
  children: React.ReactNode
  dot?: boolean
}

const styles: Record<BadgeVariant, { bg: string; border: string; color: string }> = {
  ok:      { bg: 'var(--teal-bg)',              border: 'var(--teal-dim)',              color: 'var(--teal)' },
  warn:    { bg: 'var(--amber-bg)',             border: 'var(--amber-dim)',             color: 'var(--amber)' },
  err:     { bg: 'var(--red-bg)',               border: 'var(--red-dim)',               color: 'var(--red)' },
  info:    { bg: 'var(--blue-bg)',              border: 'var(--blue-dim)',              color: 'var(--blue)' },
  // --cobalt-bg / --cobalt-dim not yet defined in the styleguide token set; using
  // computed rgba until those tokens are added to unified-styleguide.html.
  cobalt:  { bg: 'var(--cobalt-bg, rgba(56,5,227,0.12))',  border: 'var(--cobalt-dim, rgba(56,5,227,0.3))',  color: 'var(--cobalt-mid)' },
  neon:    { bg: 'var(--neon-bg, rgba(179,252,79,0.1))',   border: 'var(--neon-dim, rgba(179,252,79,0.3))',  color: 'var(--neon-green)' },
  purple:  { bg: 'var(--purple-bg)',            border: 'var(--purple-dim)',            color: 'var(--purple)' },
  neutral: { bg: 'var(--bg-raised)',            border: 'var(--border)',                color: 'var(--text-2)' },
}

export default function Badge({ variant = 'neutral', children, dot }: BadgeProps) {
  const s = styles[variant]
  return (
    <span
      className="inline-flex items-center gap-1 rounded-full px-[9px] py-[2px] text-[10.5px] font-semibold whitespace-nowrap"
      style={{
        fontFamily: 'var(--font-mono)',
        letterSpacing: '0.3px',
        background: s.bg,
        border: `1px solid ${s.border}`,
        color: s.color,
      }}
    >
      {dot && (
        <span
          className="w-1.5 h-1.5 rounded-full flex-shrink-0"
          style={{ background: s.color }}
        />
      )}
      {children}
    </span>
  )
}

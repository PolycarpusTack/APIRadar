interface PageHeaderProps {
  tag: string
  title: string
  titleAccent?: string
  description: string
  actions?: React.ReactNode
}

export default function PageHeader({ tag, title, titleAccent, description, actions }: PageHeaderProps) {
  return (
    <header
      className="relative overflow-hidden border-b bg-[var(--bg-surface)] px-14 pt-14 pb-12"
      style={{ borderColor: 'var(--border)' }}
    >
      {/* Cobalt radial glow — top right */}
      <div
        className="pointer-events-none absolute -top-20 -right-20 w-96 h-96 rounded-full"
        style={{ background: 'radial-gradient(circle, rgba(56,5,227,0.08) 0%, transparent 70%)' }}
      />
      {/* Neon radial glow — bottom centre-left */}
      <div
        className="pointer-events-none absolute -bottom-16 left-[30%] w-72 h-72 rounded-full"
        style={{ background: 'radial-gradient(circle, rgba(179,252,79,0.04) 0%, transparent 70%)' }}
      />

      <div className="relative flex items-start justify-between gap-6">
        <div>
          <p
            className="mb-[10px] text-[10.5px] font-medium uppercase tracking-[1.5px]"
            style={{ fontFamily: 'var(--font-mono)', color: 'var(--cobalt-mid)' }}
          >
            {tag}
          </p>
          <h1
            className="mb-3 text-[40px] font-bold leading-[1.1] tracking-[-1.5px]"
            style={{ fontFamily: 'var(--font-head)', color: 'var(--text-1)' }}
          >
            {title}
            {titleAccent && (
              <span style={{ color: 'var(--cobalt-mid)' }}> {titleAccent}</span>
            )}
          </h1>
          <p
            className="max-w-[640px] text-[15px] leading-[1.65]"
            style={{ color: 'var(--text-2)' }}
          >
            {description}
          </p>
        </div>
        {actions && <div className="flex-shrink-0 pt-10">{actions}</div>}
      </div>
    </header>
  )
}

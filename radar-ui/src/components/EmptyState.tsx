import type { LucideIcon } from 'lucide-react'

interface EmptyStateProps {
  icon: LucideIcon
  title: string
  description: string
  action?: React.ReactNode
}

export default function EmptyState({ icon: Icon, title, description, action }: EmptyStateProps) {
  return (
    <div className="flex flex-col items-center justify-center py-20 px-8 text-center">
      <div
        className="mb-4 flex h-12 w-12 items-center justify-center rounded-xl"
        style={{ background: 'var(--bg-raised)', border: '1px solid var(--border)' }}
      >
        <Icon className="h-5 w-5" style={{ color: 'var(--text-3)' }} />
      </div>
      <p
        className="mb-1.5 text-sm font-semibold"
        style={{ fontFamily: 'var(--font-head)', color: 'var(--text-2)' }}
      >
        {title}
      </p>
      <p className="mb-5 max-w-xs text-[12.5px] leading-relaxed" style={{ color: 'var(--text-3)' }}>
        {description}
      </p>
      {action}
    </div>
  )
}

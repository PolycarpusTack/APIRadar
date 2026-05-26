import { useNavigate } from 'react-router-dom'
import { Server, GitCompare, Users, ArrowRight, CheckCircle2 } from 'lucide-react'

interface Step {
  num: number
  icon: React.ElementType
  title: string
  description: string
  action: string
  href: string
}

const STEPS: Step[] = [
  {
    num: 1,
    icon: Server,
    title: 'Register a Service',
    description: 'Tell Radar which API you want to monitor — name, team, and spec format.',
    action: 'Go to Services',
    href: '/services',
  },
  {
    num: 2,
    icon: GitCompare,
    title: 'Compare Your Specs',
    description: 'Paste two versions of your spec and Radar will detect every breaking change instantly.',
    action: 'Compare Specs',
    href: '/diffs?compare=open',
  },
  {
    num: 3,
    icon: Users,
    title: 'Register Your Consumers',
    description: 'Add the teams and services that depend on your API so they appear in blast-radius reports.',
    action: 'Add Consumers',
    href: '/consumers',
  },
]

export default function FirstRunBanner() {
  const navigate = useNavigate()

  return (
    <div
      className="rounded-xl p-6"
      style={{ background: 'var(--bg-surface)', border: '1px solid var(--border)' }}
    >
      {/* Header */}
      <div className="flex items-start justify-between mb-5">
        <div>
          <div className="flex items-center gap-2 mb-1">
            <CheckCircle2 className="h-4 w-4" style={{ color: 'var(--cobalt-mid)' }} />
            <p className="text-[11px] font-semibold uppercase tracking-[0.9px]" style={{ color: 'var(--cobalt-mid)' }}>
              Get Started
            </p>
          </div>
          <h2 className="text-[16px] font-bold" style={{ fontFamily: 'var(--font-head)', color: 'var(--text-1)' }}>
            Set up Radar in 3 steps
          </h2>
          <p className="mt-1 text-[12.5px]" style={{ color: 'var(--text-3)' }}>
            No CLI required — you can complete the setup entirely from this dashboard.
          </p>
        </div>
      </div>

      {/* Steps */}
      <div className="grid grid-cols-3 gap-4">
        {STEPS.map((step, idx) => {
          const Icon = step.icon
          return (
            <div key={step.num} className="flex gap-3">
              {/* Step number + connector */}
              <div className="flex flex-col items-center">
                <div
                  className="flex h-7 w-7 flex-shrink-0 items-center justify-center rounded-full text-[11px] font-bold"
                  style={{
                    background: 'var(--cobalt-mid)',
                    color: '#fff',
                  }}
                >
                  {step.num}
                </div>
                {idx < STEPS.length - 1 && (
                  <div
                    className="mt-2 flex-1 w-px"
                    style={{ background: 'var(--border)', minHeight: '1.5rem' }}
                  />
                )}
              </div>

              {/* Card */}
              <div
                className="flex-1 rounded-lg p-4 cursor-pointer transition-colors hover:bg-[var(--bg-hover)]"
                style={{ border: '1px solid var(--border)', background: 'var(--bg-raised)' }}
                onClick={() => navigate(step.href)}
              >
                <div className="flex items-center gap-2 mb-1.5">
                  <Icon className="h-3.5 w-3.5" style={{ color: 'var(--cobalt-mid)' }} />
                  <p className="text-[12.5px] font-semibold" style={{ color: 'var(--text-1)' }}>
                    {step.title}
                  </p>
                </div>
                <p className="text-[11.5px] leading-relaxed mb-3" style={{ color: 'var(--text-3)' }}>
                  {step.description}
                </p>
                <button
                  className="flex items-center gap-1 text-[11.5px] font-medium transition-opacity hover:opacity-80"
                  style={{ color: 'var(--cobalt-mid)' }}
                  onClick={(e) => { e.stopPropagation(); navigate(step.href) }}
                >
                  {step.action}
                  <ArrowRight className="h-3 w-3" />
                </button>
              </div>
            </div>
          )
        })}
      </div>
    </div>
  )
}

import { FileText } from 'lucide-react'
import PageHeader from '../components/PageHeader'
import EmptyState from '../components/EmptyState'

export default function ReleaseNotesPage() {
  return (
    <div>
      <PageHeader
        tag="Docs"
        title="Release Notes"
        description="AI-generated release notes for each schema diff — including breaking change summaries, migration checklists, and per-consumer impact."
      />

      <div className="px-14 py-8">
        <div
          className="overflow-hidden rounded-lg"
          style={{ border: '1px solid var(--border)', background: 'var(--bg-surface)' }}
        >
          <div className="px-4 py-3">
            <p
              className="text-[11px] font-semibold uppercase tracking-[0.8px]"
              style={{ color: 'var(--text-3)' }}
            >
              Generated notes
            </p>
          </div>
          <EmptyState
            icon={FileText}
            title="No release notes yet"
            description="Run radar explain --diff-id <id> --release-notes to generate release notes for a diff and see them here."
          />
        </div>
      </div>
    </div>
  )
}

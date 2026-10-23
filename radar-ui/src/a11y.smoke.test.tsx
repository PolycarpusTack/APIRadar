// @vitest-environment jsdom
//
// N-22-T1 — accessibility smoke check across the main pages.
//
// Runs the axe ruleset over each page as it first renders. This is the net
// under the targeted per-page tests: it catches a control that silently loses
// its accessible name, without anyone having to write a bespoke assertion for
// that control first.
import '@testing-library/jest-dom/vitest'
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import { render, cleanup, act } from '@testing-library/react'
import { MemoryRouter } from 'react-router-dom'
import { findA11yViolations, formatViolations } from './lib/axe'

const { mockApi } = vi.hoisted(() => ({
  mockApi: { get: vi.fn(), post: vi.fn(), put: vi.fn(), patch: vi.fn(), del: vi.fn() },
}))
vi.mock('./lib/apiClient', () => ({
  api: mockApi,
  ApiError: class ApiError extends Error {
    status: number
    body?: unknown
    constructor(status: number, message: string, body?: unknown) {
      super(message)
      this.status = status
      this.body = body
    }
  },
}))

import HomePage from './pages/HomePage'
import ConsumersPage from './pages/ConsumersPage'
import ServicesPage from './pages/ServicesPage'
import DiffsPage from './pages/DiffsPage'
import EvidenceCoveragePage from './pages/EvidenceCoveragePage'
import EvolutionRulesPage from './pages/EvolutionRulesPage'
import CatalogSourcesPage from './pages/CatalogSourcesPage'
import AuditPage from './pages/AuditPage'
import ReleaseNotesPage from './pages/ReleaseNotesPage'
import GenerateTestsPage from './pages/GenerateTestsPage'
import PlaygroundPage from './pages/PlaygroundPage'
import SettingsPage from './pages/SettingsPage'
import CsvRunnerPanel from './components/CsvRunnerPanel'

/**
 * Route a mocked GET by path to a payload of the shape the page expects.
 * Everything is empty — an empty-state render is enough to exercise the page
 * chrome, its filter controls and its always-visible forms, which is where the
 * accessible-name defects live.
 */
function payloadFor(path: string): unknown {
  if (path.startsWith('/health')) return { status: 'ok' }
  if (path.startsWith('/v1/summary')) {
    return { breaking_changes_30d: 0, consumers_at_risk: 0, services_count: 0 }
  }
  if (path.startsWith('/v1/readiness')) return { overall: 'ready', items: [] }
  if (path.startsWith('/v1/settings/integrations')) {
    return {
      anthropic: false, openai: false, openai_enterprise: false,
      github_copilot: false, jira: false, github: false, postman: false,
    }
  }
  if (path.startsWith('/v1/settings')) {
    return {
      policy_block_on: 'active_consumers',
      policy_lookback_days: 30,
      policy_allow_override_with: null,
      retention_days: 90,
    }
  }
  if (path.startsWith('/scalar/version')) {
    return { bundled: '1.0.0', override: null, active: '1.0.0', latest: null, update_available: false }
  }
  // `{ entries: [] }` endpoints.
  if (
    path.startsWith('/v1/policy-decisions') ||
    path.startsWith('/v1/acknowledgements') ||
    path.startsWith('/v1/evolution-rules') ||
    path.startsWith('/v1/catalog-sources')
  ) {
    return { entries: [] }
  }
  // Everything else is a plain list endpoint.
  return []
}

beforeEach(() => {
  for (const fn of Object.values(mockApi)) fn.mockReset()
  mockApi.get.mockImplementation((path: string) => Promise.resolve(payloadFor(path)))
})
afterEach(() => cleanup())

/** Pages rendered inside a router, keyed by the name shown in test output. */
const PAGES: Array<[string, () => React.ReactElement]> = [
  ['HomePage', () => <HomePage />],
  ['ConsumersPage', () => <ConsumersPage />],
  ['ServicesPage', () => <ServicesPage />],
  ['DiffsPage', () => <DiffsPage />],
  ['EvidenceCoveragePage', () => <EvidenceCoveragePage />],
  ['EvolutionRulesPage', () => <EvolutionRulesPage />],
  ['CatalogSourcesPage', () => <CatalogSourcesPage />],
  ['AuditPage', () => <AuditPage />],
  ['ReleaseNotesPage', () => <ReleaseNotesPage />],
  ['GenerateTestsPage', () => <GenerateTestsPage />],
  ['PlaygroundPage', () => <PlaygroundPage />],
  ['SettingsPage', () => <SettingsPage />],
  ['CsvRunnerPanel', () => <CsvRunnerPanel />],
]

describe('the axe harness itself', () => {
  // A guard that cannot fail guards nothing. This proves the harness reports a
  // real defect, so a green smoke check below means something.
  it('reports an icon-only button that has no accessible name', async () => {
    const { container } = render(
      <button type="button">
        <svg aria-hidden="true" width="16" height="16" />
      </button>,
    )

    const violations = await findA11yViolations(container)
    expect(violations.map((v) => v.id)).toContain('button-name')
  }, 30_000)

  it('reports an input whose only label is a detached <label>', async () => {
    const { container } = render(
      <div>
        <label>Owner team</label>
        <input type="text" />
      </div>,
    )

    const violations = await findA11yViolations(container)
    expect(violations.map((v) => v.id)).toContain('label')
  }, 30_000)
})

describe('accessibility smoke check', () => {
  it.each(PAGES)('%s has no axe violations', async (_name, element) => {
    const { container } = render(<MemoryRouter>{element()}</MemoryRouter>)

    // Let the mount-time fetches resolve so the loaded tree — not the loading
    // skeleton — is what gets scanned. A macrotask tick is enough: every mount
    // fetch here is an already-resolved mock promise.
    await act(async () => { await new Promise((r) => setTimeout(r, 0)) })

    const violations = await findA11yViolations(container)
    expect(violations, `\n${formatViolations(violations)}`).toEqual([])
  }, 30_000)
})

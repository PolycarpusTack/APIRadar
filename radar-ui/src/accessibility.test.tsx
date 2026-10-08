// @vitest-environment jsdom
//
// N-22 accessibility pass — cross-cutting affordance guards.
//
// These live in one file on purpose: every case below needs a jsdom
// environment, and spinning one up per component made the suite the slowest
// thing in the repo. Page-specific a11y guards stay in that page's own test
// file (see AuditPage.test.tsx, EvolutionRulesPage.test.tsx).
import '@testing-library/jest-dom/vitest'
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import { render, screen, fireEvent, cleanup } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { MemoryRouter, Routes, Route } from 'react-router-dom'
import BatchComparePanel from './components/BatchComparePanel'
import RegisterConsumerForm from './components/RegisterConsumerForm'
import TermTooltip, { TERM_DEFINITIONS } from './components/TermTooltip'
import CatalogSourcesPage from './pages/CatalogSourcesPage'

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

beforeEach(() => {
  for (const fn of Object.values(mockApi)) fn.mockReset()
})
afterEach(() => cleanup())

// ---------------------------------------------------------------------------
// BatchComparePanel — clickable result rows
// ---------------------------------------------------------------------------

const CSV = [
  'label,base_url,head_url',
  'Payments,https://a.example.com/openapi.yaml,https://b.example.com/openapi.yaml',
  'Shipping,https://a.example.com/ship.yaml,https://b.example.com/ship.yaml',
].join('\n')

const RESULTS = [
  { label: 'Payments', status: 'done', diff_id: 'diff-9', breaking_count: 1, changes_count: 3 },
  { label: 'Shipping', status: 'error', breaking_count: 0, changes_count: 0, error: 'fetch failed' },
]

function renderPanel(onClose?: () => void) {
  return render(
    <MemoryRouter initialEntries={['/']}>
      <Routes>
        <Route path="/" element={<BatchComparePanel onClose={onClose} />} />
        <Route path="/diffs/:id" element={<p>diff detail page</p>} />
      </Routes>
    </MemoryRouter>,
  )
}

/** Paste the CSV and run the batch, resolving to RESULTS. */
async function runBatch() {
  mockApi.post.mockResolvedValue(RESULTS)
  renderPanel()
  fireEvent.change(screen.getByLabelText(/Paste or upload CSV/i), { target: { value: CSV } })
  await userEvent.click(screen.getByRole('button', { name: /Run Batch/i }))
  return await screen.findByRole('button', { name: 'View diff for Payments' }, { timeout: 10_000 })
}

// The result rows were clickable `<tr onClick>` with no role, no tabIndex and
// no key handler, so a keyboard user could not reach — let alone open — a diff.
describe('BatchComparePanel result rows', () => {
  it('exposes a navigable row as a named button and opens it with Enter', async () => {
    const row = await runBatch()

    row.focus()
    expect(row).toHaveFocus()
    await userEvent.keyboard('{Enter}')

    expect(await screen.findByText('diff detail page')).toBeInTheDocument()
  }, 20_000)

  it('opens it with Space as well', async () => {
    const row = await runBatch()

    row.focus()
    await userEvent.keyboard(' ')

    expect(await screen.findByText('diff detail page')).toBeInTheDocument()
  }, 20_000)

  it('leaves a failed row out of the tab order — there is nothing to open', async () => {
    await runBatch()

    expect(screen.queryByRole('button', { name: /View diff for Shipping/ })).toBeNull()
  }, 20_000)

  it('names the icon-only close button and labels the CSV field', async () => {
    const onClose = vi.fn()
    renderPanel(onClose)

    await userEvent.click(screen.getByRole('button', { name: /Close batch compare/i }))
    expect(onClose).toHaveBeenCalledOnce()
    expect(screen.getByLabelText(/Paste or upload CSV/i).tagName).toBe('TEXTAREA')
  }, 20_000)

  it('announces a run failure as an alert', async () => {
    mockApi.post.mockRejectedValue(new Error('batch service down'))
    renderPanel()
    fireEvent.change(screen.getByLabelText(/Paste or upload CSV/i), { target: { value: CSV } })
    await userEvent.click(screen.getByRole('button', { name: /Run Batch/i }))

    expect(await screen.findByRole('alert', undefined, { timeout: 10_000 }))
      .toHaveTextContent(/batch service down/i)
  }, 20_000)
})

// ---------------------------------------------------------------------------
// TermTooltip — the popover was never linked to its trigger
// ---------------------------------------------------------------------------

describe('TermTooltip', () => {
  it('describes its trigger with the popover once the popover is shown', async () => {
    render(<TermTooltip term="blast_radius" />)

    const trigger = screen.getByRole('button', { name: /Definition: blast radius/i })
    expect(trigger).not.toHaveAttribute('aria-describedby')

    await userEvent.hover(trigger)

    const tooltip = await screen.findByRole('tooltip')
    expect(tooltip).toHaveTextContent(TERM_DEFINITIONS.blast_radius)
    expect(tooltip.id).not.toBe('')
    expect(trigger).toHaveAttribute('aria-describedby', tooltip.id)
  }, 20_000)

  it('drops the description again when the popover closes', async () => {
    render(<TermTooltip term="fail_mode" />)
    const trigger = screen.getByRole('button', { name: /Definition: fail mode/i })

    await userEvent.hover(trigger)
    expect(trigger).toHaveAttribute('aria-describedby')

    await userEvent.unhover(trigger)
    expect(screen.queryByRole('tooltip')).toBeNull()
    expect(trigger).not.toHaveAttribute('aria-describedby')
  }, 20_000)
})

// ---------------------------------------------------------------------------
// RegisterConsumerForm — unassociated labels, bare icon close button
// ---------------------------------------------------------------------------

describe('RegisterConsumerForm', () => {
  const SERVICES = [{ id: 'svc-1', name: 'payments-api' }]

  function renderForm(onClose = vi.fn()) {
    mockApi.get.mockResolvedValue(SERVICES)
    render(<RegisterConsumerForm onCreated={vi.fn()} onClose={onClose} />)
    return onClose
  }

  it('gives every field an accessible name from its label', () => {
    renderForm()

    fireEvent.change(screen.getByLabelText(/Consumer Name/i), { target: { value: 'billing-service' } })
    fireEvent.change(screen.getByLabelText(/Owner Team/i), { target: { value: 'Platform' } })
    fireEvent.change(screen.getByLabelText(/Contact Email/i), { target: { value: 'team@example.com' } })
    fireEvent.change(screen.getByLabelText(/Repository URL/i), { target: { value: 'https://github.com/org/repo' } })

    expect(screen.getByLabelText(/Consumer Name/i)).toHaveValue('billing-service')
    expect(screen.getByLabelText(/Owner Team/i)).toHaveValue('Platform')
    expect(screen.getByLabelText(/Contact Email/i)).toHaveValue('team@example.com')
    expect(screen.getByLabelText(/Repository URL/i)).toHaveValue('https://github.com/org/repo')
  }, 20_000)

  it('names the icon-only close button', async () => {
    const onClose = renderForm()

    await userEvent.click(screen.getByRole('button', { name: /Close consumer registration/i }))
    expect(onClose).toHaveBeenCalledOnce()
  }, 20_000)

  it('exposes the subscription toggles as a named, pressable group', async () => {
    renderForm()

    const group = await screen.findByRole('group', { name: /Subscribe to Services/i }, { timeout: 10_000 })
    const toggle = screen.getByRole('button', { name: 'payments-api' })
    expect(group).toContainElement(toggle)
    expect(toggle).toHaveAttribute('aria-pressed', 'false')

    await userEvent.click(toggle)
    expect(screen.getByRole('button', { name: 'payments-api' })).toHaveAttribute('aria-pressed', 'true')
  }, 20_000)

  it('announces a submit failure as an alert', async () => {
    mockApi.post.mockRejectedValue(new Error('consumer service down'))
    renderForm()

    fireEvent.change(screen.getByLabelText(/Consumer Name/i), { target: { value: 'billing-service' } })
    fireEvent.change(screen.getByLabelText(/Owner Team/i), { target: { value: 'Platform' } })
    fireEvent.change(screen.getByLabelText(/Contact Email/i), { target: { value: 'team@example.com' } })
    await userEvent.click(screen.getByRole('button', { name: /^Register$/ }))

    expect(await screen.findByRole('alert', undefined, { timeout: 10_000 }))
      .toHaveTextContent(/consumer service down/i)
  }, 20_000)
})

// ---------------------------------------------------------------------------
// CatalogSourcesPage — five unassociated labels, bare icon close button
// ---------------------------------------------------------------------------

describe('CatalogSourcesPage', () => {
  function renderPage() {
    return render(<MemoryRouter><CatalogSourcesPage /></MemoryRouter>)
  }

  it('gives every create-form control an accessible name from its label', async () => {
    mockApi.get.mockResolvedValue({ entries: [] })
    renderPage()

    await userEvent.click(await screen.findByRole('button', { name: /Add source/i }, { timeout: 10_000 }))

    expect(screen.getByLabelText(/^Kind$/i)).toHaveValue('backstage')
    fireEvent.change(screen.getByLabelText(/^Name$/i), { target: { value: 'Internal Backstage' } })
    expect(screen.getByLabelText(/^Name$/i)).toHaveValue('Internal Backstage')
    expect(screen.getByLabelText(/^URL$/i).tagName).toBe('INPUT')
    expect(screen.getByLabelText(/Token env var/i).tagName).toBe('INPUT')
    expect(screen.getByLabelText(/Sync interval/i)).toHaveValue(3600)
  }, 20_000)

  it('names the icon-only close button', async () => {
    mockApi.get.mockResolvedValue({ entries: [] })
    renderPage()

    await userEvent.click(await screen.findByRole('button', { name: /Add source/i }, { timeout: 10_000 }))
    await userEvent.click(screen.getByRole('button', { name: /Close new catalog source form/i }))

    expect(screen.queryByLabelText(/Token env var/i)).toBeNull()
  }, 20_000)

  it('announces a load failure as an alert', async () => {
    mockApi.get.mockRejectedValue(new Error('catalog unavailable'))
    renderPage()

    expect(await screen.findByRole('alert', undefined, { timeout: 10_000 }))
      .toHaveTextContent(/Failed to load catalog sources: .*catalog unavailable/)
  }, 20_000)
})

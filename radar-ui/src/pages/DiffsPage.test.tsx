// @vitest-environment jsdom
import '@testing-library/jest-dom/vitest'
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import { render, screen, waitFor, cleanup, within } from '@testing-library/react'
import { MemoryRouter } from 'react-router-dom'
import DiffsPage from './DiffsPage'

// Shared api mock (hoisted so the vi.mock factory can reference it).
const { mockApi } = vi.hoisted(() => ({
  mockApi: { get: vi.fn(), post: vi.fn(), put: vi.fn(), patch: vi.fn(), del: vi.fn() },
}))
vi.mock('../lib/apiClient', () => ({
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

function makeDiff(i: number, serviceId = `svc-${i}`, serviceName = `service-${i}`) {
  return {
    id: `diff-${i}`,
    service_id: serviceId,
    service_name: serviceName,
    from_git_ref: 'abc1234567890',
    to_git_ref: 'def1234567890',
    pr_url: null,
    created_at: '2026-06-01T12:00:00Z',
    breaking_count: 0,
    risky_count: 0,
    safe_count: 1,
  }
}

function renderAt(path: string) {
  return render(
    <MemoryRouter initialEntries={[path]}>
      <DiffsPage />
    </MemoryRouter>,
  )
}

beforeEach(() => {
  mockApi.get.mockReset()
})
afterEach(() => cleanup())

describe('DiffsPage pagination', () => {
  it('enables Next when a full page (50) is returned and disables Previous on page 1', async () => {
    const rows = Array.from({ length: 50 }, (_, i) => makeDiff(i))
    mockApi.get.mockImplementation(async (path: string) => {
      expect(path).toContain('limit=50')
      expect(path).toContain('offset=0')
      return rows
    })

    renderAt('/diffs')

    // Wait for the table to populate (count label shows the range "Showing 1–50").
    await screen.findByText(/Showing 1/)

    const next = screen.getByRole('button', { name: /next/i })
    const prev = screen.getByRole('button', { name: /previous/i })
    expect(next).toBeEnabled()
    expect(prev).toBeDisabled()
  })
})

describe('DiffsPage ?service= filter', () => {
  it('fetches a wider window, filters to the service, and hides pagination', async () => {
    const rows = [
      makeDiff(1, 'svc-1', 'payments-api'),
      makeDiff(2, 'svc-2', 'shipping-api'),
      makeDiff(3, 'svc-1', 'payments-api'),
    ]
    mockApi.get.mockImplementation(async (path: string) => {
      // Filter mode scans a wider window (limit=200) client-side.
      expect(path).toContain('limit=200')
      return rows
    })

    renderAt('/diffs?service=svc-1')

    // Two rows match svc-1; the count label reflects the filtered total.
    await screen.findByText(/2 diffs/)

    const table = screen.getByRole('table')
    // Matching service is shown; the non-matching one is filtered out.
    expect(within(table).getAllByText('payments-api')).toHaveLength(2)
    expect(within(table).queryByText('shipping-api')).toBeNull()

    // Pagination is not rendered in filter mode.
    expect(screen.queryByRole('button', { name: /next/i })).toBeNull()
    expect(screen.queryByRole('button', { name: /previous/i })).toBeNull()
  })

  it('shows a distinct error state when the fetch fails', async () => {
    mockApi.get.mockRejectedValue(new Error('network down'))
    renderAt('/diffs')
    await waitFor(() =>
      expect(screen.getByText(/Failed to load diffs: network down/)).toBeInTheDocument(),
    )
  })
})

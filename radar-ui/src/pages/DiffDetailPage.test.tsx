// @vitest-environment jsdom
import '@testing-library/jest-dom/vitest'
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import { render, screen, cleanup } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { MemoryRouter, Routes, Route } from 'react-router-dom'
import DiffDetailPage from './DiffDetailPage'

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

const DIFF = {
  id: 'abc',
  from_git_ref: 'v1',
  to_git_ref: 'v2',
  pr_url: null,
  created_at: '2026-06-01T00:00:00Z',
  changes: [
    { path: 'users.email', kind: 'field_removed', severity: 'breaking', description: 'removed' },
  ],
}

function baseGet(overrides: Record<string, unknown> = {}) {
  return async (path: string) => {
    if (path === '/v1/diffs/abc') return DIFF
    if (path === '/v1/diffs/abc/blast-radius') return { diff_id: 'abc', service_id: 's', lookback_days: 30, entries: [] }
    if (path === '/v1/diffs/abc/acknowledgements') return { entries: [] }
    if (path in overrides) return overrides[path]
    throw new Error(`unexpected GET ${path}`)
  }
}

function renderPage() {
  return render(
    <MemoryRouter initialEntries={['/diffs/abc']}>
      <Routes>
        <Route path="/diffs/:id" element={<DiffDetailPage />} />
      </Routes>
    </MemoryRouter>,
  )
}

beforeEach(() => {
  for (const fn of Object.values(mockApi)) fn.mockReset()
})
afterEach(() => cleanup())

describe('DiffDetailPage release-note generation', () => {
  it('polls generate-status and renders the completed note content', async () => {
    mockApi.get.mockImplementation(
      baseGet({
        '/v1/release-notes/note-1/generate-status': {
          generation_status: 'completed',
          content: '# Release Notes\nGenerated body.',
        },
      }),
    )
    mockApi.post.mockImplementation(async (path: string) => {
      if (path === '/v1/diffs/abc/release-notes/generate') {
        return { id: 'note-1', generation_status: 'pending' }
      }
      throw new Error(`unexpected POST ${path}`)
    })

    renderPage()

    const btn = await screen.findByRole('button', { name: /Generate Release Notes/ })
    await userEvent.click(btn)

    expect(await screen.findByText(/Generated body\./)).toBeInTheDocument()
    // The status endpoint was actually polled.
    expect(mockApi.get).toHaveBeenCalledWith('/v1/release-notes/note-1/generate-status')
  })

  it('renders the generation error when the status comes back failed', async () => {
    mockApi.get.mockImplementation(
      baseGet({
        '/v1/release-notes/note-1/generate-status': {
          generation_status: 'failed',
          generation_error: 'model timed out',
        },
      }),
    )
    mockApi.post.mockResolvedValue({ id: 'note-1', generation_status: 'pending' })

    renderPage()

    const btn = await screen.findByRole('button', { name: /Generate Release Notes/ })
    await userEvent.click(btn)

    expect(await screen.findByText(/model timed out/)).toBeInTheDocument()
  })
})

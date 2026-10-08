// @vitest-environment jsdom
//
// N-23 regression guard: the two offset-driven effects had no cancellation, so
// paginating faster than the API responds could paint an older page over a
// newer one (and left requests running after the page was gone).
import '@testing-library/jest-dom/vitest'
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import { render, screen, act, cleanup, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { MemoryRouter } from 'react-router-dom'
import AuditPage from './AuditPage'

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

interface Decision {
  id: string
  service_id: string | null
  diff_id: string | null
  verdict: string
  fail_mode: string
  actor: string | null
  created_at: string
}

/** A full page (LIMIT=25) of decisions tagged with the offset they came from. */
function page(offset: number): { entries: Decision[] } {
  return {
    entries: Array.from({ length: 25 }, (_, i) => ({
      id: `d-${offset}-${i}`,
      service_id: `svc-from-offset-${offset}`,
      diff_id: null,
      verdict: 'pass',
      fail_mode: 'block',
      actor: null,
      created_at: '2026-06-01T00:00:00Z',
    })),
  }
}

interface Deferred<T> { promise: Promise<T>; resolve: (value: T) => void }
function deferred<T>(): Deferred<T> {
  let resolve!: (value: T) => void
  const promise = new Promise<T>((res) => { resolve = res })
  return { promise, resolve }
}

function renderPage() {
  return render(<MemoryRouter><AuditPage /></MemoryRouter>)
}

beforeEach(() => {
  for (const fn of Object.values(mockApi)) fn.mockReset()
})
afterEach(() => cleanup())

describe('AuditPage pagination ordering', () => {
  it('keeps the newest page when an older page response lands last', async () => {
    const pending = new Map<number, Deferred<{ entries: Decision[] }>>()
    mockApi.get.mockImplementation((path: string) => {
      if (path.startsWith('/v1/acknowledgements')) return Promise.resolve({ entries: [] })
      const offset = Number(new URL(path, 'http://x').searchParams.get('offset'))
      if (offset === 0) return Promise.resolve(page(0))
      const d = deferred<{ entries: Decision[] }>()
      pending.set(offset, d)
      return d.promise
    })

    renderPage()
    expect(await screen.findAllByText('svc-from-offset-0', undefined, { timeout: 5000 })).toHaveLength(25)

    // Two quick "next page" clicks — the first page-turn is still in flight
    // when the second is issued.
    const next = () => screen.getByRole('button', { name: /Next page of policy decisions/i })
    await userEvent.click(next())
    await waitFor(() => expect(pending.has(25)).toBe(true))
    await userEvent.click(next())
    await waitFor(() => expect(pending.has(50)).toBe(true))

    // The newest request answers first…
    await act(async () => { pending.get(50)!.resolve(page(50)); await pending.get(50)!.promise })
    expect(await screen.findAllByText('svc-from-offset-50', undefined, { timeout: 5000 })).toHaveLength(25)

    // …then the superseded offset=25 response arrives. It must not repaint.
    await act(async () => { pending.get(25)!.resolve(page(25)); await pending.get(25)!.promise })
    expect(screen.getAllByText('svc-from-offset-50')).toHaveLength(25)
    expect(screen.queryAllByText('svc-from-offset-25')).toHaveLength(0)
  })

  it('aborts in-flight audit requests when the page unmounts', async () => {
    const signals: (AbortSignal | undefined)[] = []
    mockApi.get.mockImplementation((_path: string, opts?: { signal?: AbortSignal }) => {
      signals.push(opts?.signal)
      return new Promise(() => {})
    })

    const { unmount } = renderPage()
    expect(signals).toHaveLength(2)
    expect(signals.every(s => s instanceof AbortSignal)).toBe(true)

    unmount()

    expect(signals.every(s => s?.aborted)).toBe(true)
  })
})

describe('AuditPage error state', () => {
  it('reports a failed load rather than showing it as empty', async () => {
    mockApi.get.mockImplementation((path: string) => {
      if (path.startsWith('/v1/acknowledgements')) return Promise.resolve({ entries: [] })
      return Promise.reject(new Error('audit unavailable'))
    })

    renderPage()

    expect(await screen.findByText(/Failed to load policy decisions: audit unavailable/, undefined, { timeout: 5000 })).toBeInTheDocument()
  })

  // N-22: a load failure has to reach a screen-reader user, not just a sighted one.
  it('announces the failure as an alert', async () => {
    mockApi.get.mockImplementation((path: string) => {
      if (path.startsWith('/v1/acknowledgements')) return Promise.resolve({ entries: [] })
      return Promise.reject(new Error('audit unavailable'))
    })

    renderPage()

    expect(await screen.findByRole('alert', undefined, { timeout: 10_000 }))
      .toHaveTextContent(/Failed to load policy decisions/)
  }, 20_000)
})

// N-22: both lists are paged by the same icon-only chevrons. Without an
// accessible name they were announced as "button, button, button, button".
describe('AuditPage pagination accessibility', () => {
  it('gives each pager a distinct accessible name', async () => {
    mockApi.get.mockImplementation((path: string) => {
      if (path.startsWith('/v1/acknowledgements')) {
        return Promise.resolve({
          entries: [{
            id: 'ack-1', diff_id: null, service_id: null, consumer_id: null,
            acknowledged_by: 'alice', reason: 'known', expires_at: null,
            created_at: '2026-06-01T00:00:00Z',
          }],
        })
      }
      return Promise.resolve(page(0))
    })

    renderPage()

    await screen.findByRole('button', { name: /Next page of policy decisions/i }, { timeout: 10_000 })
    expect(screen.getByRole('button', { name: /Previous page of policy decisions/i })).toBeDisabled()
    expect(screen.getByRole('button', { name: /Next page of policy decisions/i })).toBeEnabled()

    // The acknowledgements pager is a separate, separately-named control.
    expect(screen.getByRole('button', { name: /Previous page of acknowledgements/i })).toBeInTheDocument()
    expect(screen.getByRole('button', { name: /Next page of acknowledgements/i })).toBeDisabled()
  }, 20_000)
})

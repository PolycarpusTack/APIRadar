// @vitest-environment jsdom
//
// N-23 regression guard: navigating consumer A → consumer B fired a second
// unguarded fetch chain. Whichever response landed last won, so a slow response
// for A could render over B's data.
import '@testing-library/jest-dom/vitest'
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import { render, screen, act, cleanup, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { MemoryRouter, Routes, Route, useNavigate } from 'react-router-dom'
import ConsumerDetailPage from './ConsumerDetailPage'

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

const CONSUMERS = [
  { id: 'a', name: 'Consumer Alpha', repo_url: '', owner_team: '', contact: '', subscription_count: 0, last_seen: null },
  { id: 'b', name: 'Consumer Bravo', repo_url: '', owner_team: '', contact: '', subscription_count: 0, last_seen: null },
]

interface Deferred<T> { promise: Promise<T>; resolve: (value: T) => void }
function deferred<T>(): Deferred<T> {
  let resolve!: (value: T) => void
  const promise = new Promise<T>((res) => { resolve = res })
  return { promise, resolve }
}

function NavToBravo() {
  const navigate = useNavigate()
  return <button type="button" onClick={() => navigate('/consumers/b')}>go to bravo</button>
}

function renderAt(id: string) {
  return render(
    <MemoryRouter initialEntries={[`/consumers/${id}`]}>
      <Routes>
        <Route
          path="/consumers/:id"
          element={<><NavToBravo /><ConsumerDetailPage /></>}
        />
      </Routes>
    </MemoryRouter>,
  )
}

beforeEach(() => {
  for (const fn of Object.values(mockApi)) fn.mockReset()
})
afterEach(() => cleanup())

describe('ConsumerDetailPage stale-response safety', () => {
  it('does not render the previous consumer when its response lands last', async () => {
    const pending: Deferred<typeof CONSUMERS>[] = []
    mockApi.get.mockImplementation((path: string) => {
      if (path === '/v1/consumers') {
        const d = deferred<typeof CONSUMERS>()
        pending.push(d)
        return d.promise
      }
      if (path === '/v1/services') return Promise.resolve([])
      return Promise.reject(new Error(`unexpected GET ${path}`))
    })

    renderAt('a')
    expect(pending).toHaveLength(1)

    await userEvent.click(screen.getByRole('button', { name: 'go to bravo' }))
    await waitFor(() => expect(pending).toHaveLength(2))

    // Bravo's request (the current one) resolves first.
    await act(async () => { pending[1].resolve(CONSUMERS); await pending[1].promise })
    expect(screen.getByRole('heading', { name: 'Consumer Bravo' })).toBeInTheDocument()

    // Alpha's stale response lands afterwards — it must be ignored.
    await act(async () => { pending[0].resolve(CONSUMERS); await pending[0].promise })
    expect(screen.getByRole('heading', { name: 'Consumer Bravo' })).toBeInTheDocument()
    expect(screen.queryAllByText('Consumer Alpha')).toHaveLength(0)
  })

  it('aborts the in-flight request when the page unmounts', async () => {
    const signals: (AbortSignal | undefined)[] = []
    const held = deferred<typeof CONSUMERS>()
    mockApi.get.mockImplementation((path: string, opts?: { signal?: AbortSignal }) => {
      signals.push(opts?.signal)
      if (path === '/v1/consumers') return held.promise
      return Promise.resolve([])
    })

    const { unmount } = renderAt('a')
    expect(signals[0]).toBeInstanceOf(AbortSignal)
    expect(signals[0]?.aborted).toBe(false)

    unmount()

    expect(signals[0]?.aborted).toBe(true)
    await act(async () => { held.resolve(CONSUMERS); await held.promise })
  })

  it('shows a load failure distinctly from an empty consumer', async () => {
    mockApi.get.mockImplementation((path: string) => {
      if (path === '/v1/consumers') return Promise.reject(new Error('api offline'))
      return Promise.resolve([])
    })

    renderAt('a')

    expect(await screen.findByText(/api offline/, undefined, { timeout: 5000 })).toBeInTheDocument()
  })
})

// @vitest-environment jsdom
import '@testing-library/jest-dom/vitest'
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import { render, screen, act, cleanup } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { MemoryRouter, Routes, Route, useNavigate } from 'react-router-dom'
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

describe('DiffDetailPage collection Evidence', () => {
  it('labels collection Evidence returned in Blast Radius', async () => {
    const blast = {
      diff_id: 'abc', service_id: 's', lookback_days: 30,
      entries: [{
        consumer: { id: 'collection-consumer', name: 'Collection Consumer', repo_url: '', owner_team: '', contact: '' },
        confidence: 'medium', last_seen: '2026-06-01T00:00:00Z',
        has_runtime_usage: false, has_call_site: false, has_collection_file: true,
      }],
    }
    mockApi.get.mockImplementation(async (path: string) => {
      if (path === '/v1/diffs/abc/blast-radius') return blast
      return baseGet()(path)
    })

    renderPage()

    expect(await screen.findByText('Collection Consumer')).toBeInTheDocument()
    expect(screen.getByText('collection file')).toBeInTheDocument()
    expect(screen.getByText('medium')).toBeInTheDocument()
  })
})

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

// ---------------------------------------------------------------------------
// N-23 — stale-response safety when navigating between diffs
// ---------------------------------------------------------------------------

interface Deferred<T> { promise: Promise<T>; resolve: (value: T) => void }
function deferred<T>(): Deferred<T> {
  let resolve!: (value: T) => void
  const promise = new Promise<T>((res) => { resolve = res })
  return { promise, resolve }
}

const DIFF_DEF = {
  id: 'def',
  from_git_ref: 'v9',
  to_git_ref: 'v10',
  pr_url: null,
  created_at: '2026-06-02T00:00:00Z',
  changes: [],
}

function NavToDef() {
  const navigate = useNavigate()
  return <button type="button" onClick={() => navigate('/diffs/def')}>go to def</button>
}

function renderWithNav(startId: string) {
  return render(
    <MemoryRouter initialEntries={[`/diffs/${startId}`]}>
      <Routes>
        <Route path="/diffs/:id" element={<><NavToDef /><DiffDetailPage /></>} />
      </Routes>
    </MemoryRouter>,
  )
}

describe('DiffDetailPage stale-response safety', () => {
  it('does not render the previous diff when its response lands last', async () => {
    const slowAbc = deferred<typeof DIFF>()
    mockApi.get.mockImplementation((path: string) => {
      if (path === '/v1/diffs/abc') return slowAbc.promise
      if (path === '/v1/diffs/abc/blast-radius') return Promise.resolve({ diff_id: 'abc', service_id: 's', lookback_days: 30, entries: [] })
      if (path === '/v1/diffs/def') return Promise.resolve(DIFF_DEF)
      if (path === '/v1/diffs/def/blast-radius') return Promise.resolve({ diff_id: 'def', service_id: 's', lookback_days: 30, entries: [] })
      if (path.endsWith('/acknowledgements')) return Promise.resolve({ entries: [] })
      return Promise.reject(new Error(`unexpected GET ${path}`))
    })

    renderWithNav('abc')

    await userEvent.click(screen.getByRole('button', { name: 'go to def' }))
    // Let def's Promise.all chain settle.
    await act(async () => { for (let i = 0; i < 5; i++) await Promise.resolve() })
    expect(screen.getByText('v10')).toBeInTheDocument()

    // The superseded request for `abc` answers afterwards — it must be ignored.
    await act(async () => { slowAbc.resolve(DIFF); await slowAbc.promise })

    expect(screen.getByText('v10')).toBeInTheDocument()
    expect(screen.queryByText('v2')).not.toBeInTheDocument()
  })

  it('aborts the in-flight diff requests when the page unmounts', async () => {
    const signals: (AbortSignal | undefined)[] = []
    mockApi.get.mockImplementation((_path: string, opts?: { signal?: AbortSignal }) => {
      signals.push(opts?.signal)
      return new Promise(() => {})
    })

    const { unmount } = renderPage()
    expect(signals.length).toBeGreaterThan(0)
    expect(signals.every(s => s instanceof AbortSignal)).toBe(true)

    unmount()

    expect(signals.every(s => s?.aborted)).toBe(true)
  })
})

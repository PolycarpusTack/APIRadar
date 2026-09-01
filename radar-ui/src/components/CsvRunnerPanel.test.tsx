// @vitest-environment jsdom
import '@testing-library/jest-dom/vitest'
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import { render, screen, fireEvent, waitFor, cleanup } from '@testing-library/react'
import CsvRunnerPanel from './CsvRunnerPanel'

// Shared api mock (hoisted so the vi.mock factory can reference it).
const { mockApi, mockTriggerDownload } = vi.hoisted(() => ({
  mockApi: { get: vi.fn(), post: vi.fn(), put: vi.fn(), patch: vi.fn(), del: vi.fn() },
  mockTriggerDownload: vi.fn(),
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
// Keep the real CSV exporter logic but intercept the download so we can read
// the Blob that would have been saved (jsdom has no URL.createObjectURL).
vi.mock('../lib/csvExporter', async importOriginal => {
  const actual = await importOriginal<typeof import('../lib/csvExporter')>()
  return { ...actual, triggerDownload: mockTriggerDownload }
})

/** Load a CSV into the hidden file input and wait for it to be parsed. */
async function uploadCsv(csvText: string, rowCount: number) {
  const input = document.querySelector<HTMLInputElement>('input[type="file"]')!
  const file = new File([csvText], 'data.csv', { type: 'text/csv' })
  fireEvent.change(input, { target: { files: [file] } })
  await screen.findByText(new RegExp(`${rowCount} rows loaded`))
}

function setUrl(url: string) {
  const urlInput = screen.getByPlaceholderText('https://api.example.com/users/{{user_id}}')
  fireEvent.change(urlInput, { target: { value: url } })
}

beforeEach(() => {
  mockApi.get.mockReset()
  mockApi.post.mockReset()
  mockTriggerDownload.mockReset()
})
afterEach(() => cleanup())

describe('CsvRunnerPanel run options', () => {
  it('posts the current checkbox values when toggled after the CSV is loaded', async () => {
    mockApi.get.mockResolvedValue([]) // run history on mount
    mockApi.post.mockResolvedValue({ id: 'job-1', status: 'pending', total_rows: 2 })

    render(<CsvRunnerPanel />)

    setUrl('https://api.example.test/users/{{user_id}}')
    // POST so the retry checkbox is rendered (GET/HEAD always retry server-side).
    const methodSelect = screen.getByDisplayValue('GET')
    fireEvent.change(methodSelect, { target: { value: 'POST' } })
    await uploadCsv('user_id\nalice\ncarol', 2)

    // Toggle both options AFTER rows/request settled — this must reach the POST.
    fireEvent.click(screen.getByLabelText(/Capture response body/))
    fireEvent.click(screen.getByLabelText(/Retry on 5xx failure/))
    fireEvent.click(screen.getByRole('button', { name: /Run Batch/ }))

    await waitFor(() => expect(mockApi.post).toHaveBeenCalledTimes(1))
    expect(mockApi.post).toHaveBeenCalledWith(
      '/v1/csv-runs',
      expect.objectContaining({
        request: expect.objectContaining({ capture_body: true, enable_retry: true }),
      }),
    )
  })
})

describe('CsvRunnerPanel historical runs', () => {
  it('exports failed rows from the run\'s persisted row_data (by row_number), not the CSV currently loaded', async () => {
    const runB = {
      id: 'run-b',
      name: 'Run B',
      status: 'completed_with_failures',
      total_rows: 2,
      completed_rows: 2,
      error_count: 1,
      error_message: null,
      created_at: '2026-08-01T10:00:00Z',
      started_at: '2026-08-01T10:00:00Z',
      completed_at: '2026-08-01T10:00:05Z',
    }
    // Results arrive ordered by row_number descending — index pairing would
    // attach the wrong local row even for a same-shape CSV.
    const runBResults = [
      {
        row_number: 2,
        http_status: 200,
        duration_ms: 10,
        error: null,
        url: 'https://api.example.test/users/dana-from-b',
        response_body: null,
        row_data: JSON.stringify({ user_id: 'dana-from-b' }),
      },
      {
        row_number: 1,
        http_status: 500,
        duration_ms: 12,
        error: null,
        url: 'https://api.example.test/users/bob-from-b',
        response_body: null,
        row_data: JSON.stringify({ user_id: 'bob-from-b' }),
      },
    ]
    mockApi.get.mockImplementation(async (path: string) => {
      if (path === '/v1/csv-runs') return [runB]
      if (path.startsWith('/v1/csv-runs/run-b/results')) return runBResults
      throw new Error(`unexpected GET ${path}`)
    })

    render(<CsvRunnerPanel />)

    // CSV A is loaded locally (different data from run B).
    setUrl('https://api.example.test/users/{{user_id}}')
    await uploadCsv('user_id\nalice-from-a\ncarol-from-a', 2)

    // Open the historical run B from the history list.
    fireEvent.click(await screen.findByRole('button', { name: /Recent Runs/ }))
    fireEvent.click((await screen.findByText('Run B')).closest('button')!)
    await screen.findByText('completed with failures')

    // Export the failed rows of run B.
    fireEvent.click(screen.getByRole('button', { name: /Failed Rows/ }))
    expect(mockTriggerDownload).toHaveBeenCalledTimes(1)
    const blob = mockTriggerDownload.mock.calls[0][0] as Blob
    const text = await blob.text()

    // Row 1 of run B failed (500) — the export must carry run B's persisted row.
    expect(text).toContain('bob-from-b')
    expect(text).not.toContain('alice-from-a')
    expect(text).not.toContain('carol-from-a')
  })
})

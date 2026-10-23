// @vitest-environment jsdom
//
// N-22 accessibility guard: the Jira ticket field — in both its "ticket key"
// and "paste text" modes — sat under a plain <span> heading with no <label>,
// so its only accessible name was a placeholder. axe accepts a placeholder as
// a name, which is exactly why this needs a test of its own.
import '@testing-library/jest-dom/vitest'
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import { render, screen, cleanup, fireEvent } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import GenerateTestsPage from './GenerateTestsPage'

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

beforeEach(() => {
  for (const fn of Object.values(mockApi)) fn.mockReset()
  mockApi.get.mockResolvedValue([])
})
afterEach(() => cleanup())

describe('GenerateTestsPage accessibility', () => {
  it('names the Jira ticket key field', async () => {
    render(<GenerateTestsPage />)

    const key = await screen.findByLabelText(/Jira ticket key/i)
    fireEvent.change(key, { target: { value: 'PROJ-123' } })
    expect(key).toHaveValue('PROJ-123')
  }, 20_000)

  it('names the pasted Jira ticket text field after switching mode', async () => {
    render(<GenerateTestsPage />)

    await userEvent.click(await screen.findByRole('button', { name: /Switch to paste text/i }))

    const text = screen.getByLabelText(/Jira ticket text/i)
    fireEvent.change(text, { target: { value: 'Title\nDescription' } })
    expect(text).toHaveValue('Title\nDescription')
  }, 20_000)

  it('names the spec and base URL fields', async () => {
    render(<GenerateTestsPage />)

    expect(await screen.findByLabelText(/OpenAPI spec/i)).toBeInTheDocument()
    expect(screen.getByLabelText(/API base URL/i)).toHaveValue('http://localhost:8080')
  }, 20_000)
})

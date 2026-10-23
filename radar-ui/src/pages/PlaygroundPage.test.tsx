// @vitest-environment jsdom
//
// N-22 accessibility guard: the spec URL bar was a bare <input> next to a
// decorative icon — its only accessible name was a placeholder.
import '@testing-library/jest-dom/vitest'
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import { render, screen, cleanup, fireEvent } from '@testing-library/react'
import PlaygroundPage from './PlaygroundPage'

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

describe('PlaygroundPage accessibility', () => {
  it('names the spec URL field', async () => {
    render(<PlaygroundPage />)

    const url = await screen.findByLabelText(/OpenAPI spec URL/i)
    fireEvent.change(url, { target: { value: 'https://api.example.com/openapi.yaml' } })
    expect(url).toHaveValue('https://api.example.com/openapi.yaml')
  }, 20_000)
})

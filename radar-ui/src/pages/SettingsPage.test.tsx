// @vitest-environment jsdom
import '@testing-library/jest-dom/vitest'
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import { render, screen, waitFor, cleanup } from '@testing-library/react'
import SettingsPage from './SettingsPage'

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

const SETTINGS = {
  policy_block_on: 'active_consumers',
  policy_lookback_days: 30,
  policy_allow_override_with: null,
  retention_days: 90,
}
const INTEGRATIONS = {
  anthropic: false, openai: false, openai_enterprise: false, github_copilot: false,
  jira: false, github: false, postman: false,
}
const WEBHOOK = {
  id: 'wh-1',
  url: 'https://hooks.example.com/radar',
  events: ['diff.created'],
  secret_hint: 'abcd…',
  active: true,
  created_at: '2026-06-01T00:00:00Z',
}

beforeEach(() => {
  for (const fn of Object.values(mockApi)) fn.mockReset()
})
afterEach(() => cleanup())

describe('SettingsPage destructive/action buttons', () => {
  it('renders webhook test/delete controls as type="button" so they never submit a form', async () => {
    mockApi.get.mockImplementation(async (path: string) => {
      if (path === '/v1/settings') return SETTINGS
      if (path === '/v1/settings/integrations') return INTEGRATIONS
      if (path === '/v1/webhooks') return [WEBHOOK]
      if (path === '/v1/scheduled-scans') return []
      throw new Error(`unexpected GET ${path}`)
    })

    render(<SettingsPage />)

    const del = await screen.findByRole('button', { name: /Delete webhook https:\/\/hooks\.example\.com\/radar/ })
    const test = screen.getByRole('button', { name: /Send test ping to https:\/\/hooks\.example\.com\/radar/ })
    const save = screen.getByRole('button', { name: /Save settings/ })

    expect(del).toHaveAttribute('type', 'button')
    expect(test).toHaveAttribute('type', 'button')
    expect(save).toHaveAttribute('type', 'button')
  })
})

describe('SettingsPage load-failure state', () => {
  it('shows a distinct error banner when settings fail to load', async () => {
    mockApi.get.mockImplementation(async (path: string) => {
      if (path === '/v1/settings') throw new Error('boom')
      if (path === '/v1/settings/integrations') return INTEGRATIONS
      if (path === '/v1/webhooks') return []
      if (path === '/v1/scheduled-scans') return []
      throw new Error(`unexpected GET ${path}`)
    })

    render(<SettingsPage />)

    await waitFor(() =>
      expect(screen.getByText(/Failed to load settings: boom/)).toBeInTheDocument(),
    )
  })
})

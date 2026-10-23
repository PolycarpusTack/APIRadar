// @vitest-environment jsdom
import '@testing-library/jest-dom/vitest'
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import { render, screen, waitFor, cleanup } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
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

const SCAN = {
  id: 'scan-1',
  service_id: 'svc-1',
  spec_url: 'https://api.example.com/openapi.json',
  format: 'openapi',
  interval_minutes: 60,
  last_run_at: null,
  last_run_status: null,
  last_run_error: null,
  active: true,
  created_at: '2026-06-01T00:00:00Z',
}

/** Happy-path GETs for the whole page; `overrides` replaces individual paths. */
function pageGet(overrides: Record<string, unknown> = {}) {
  return async (path: string) => {
    if (path in overrides) return overrides[path]
    if (path === '/v1/settings') return SETTINGS
    if (path === '/v1/settings/integrations') return INTEGRATIONS
    if (path === '/v1/webhooks') return [WEBHOOK]
    if (path === '/v1/scheduled-scans') return [SCAN]
    throw new Error(`unexpected GET ${path}`)
  }
}

beforeEach(() => {
  for (const fn of Object.values(mockApi)) fn.mockReset()
})
afterEach(() => {
  cleanup()
  vi.restoreAllMocks()
})

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

    const del = await screen.findByRole('button', { name: /Delete webhook https:\/\/hooks\.example\.com\/radar/ }, { timeout: 5000 })
    const test = screen.getByRole('button', { name: /Send test ping to https:\/\/hooks\.example\.com\/radar/ })
    const save = screen.getByRole('button', { name: /Save settings/ })

    expect(del).toHaveAttribute('type', 'button')
    expect(test).toHaveAttribute('type', 'button')
    expect(save).toHaveAttribute('type', 'button')
  })
})

describe('SettingsPage destructive-action safety (N-25)', () => {
  it('confirms before deleting a webhook and does nothing when dismissed', async () => {
    mockApi.get.mockImplementation(pageGet())
    const confirmSpy = vi.spyOn(window, 'confirm').mockReturnValue(false)

    render(<SettingsPage />)
    await userEvent.click(
      await screen.findByRole('button', { name: /Delete webhook https:\/\/hooks\.example\.com\/radar/ }, { timeout: 5000 }),
    )

    expect(confirmSpy).toHaveBeenCalledOnce()
    expect(mockApi.del).not.toHaveBeenCalled()
  })

  it('surfaces a failed webhook delete instead of rejecting unhandled', async () => {
    mockApi.get.mockImplementation(pageGet())
    mockApi.del.mockRejectedValue(new Error('webhook is locked'))
    vi.spyOn(window, 'confirm').mockReturnValue(true)

    render(<SettingsPage />)
    await userEvent.click(
      await screen.findByRole('button', { name: /Delete webhook https:\/\/hooks\.example\.com\/radar/ }, { timeout: 5000 }),
    )

    await waitFor(() =>
      expect(screen.getByText(/Failed to delete webhook: webhook is locked/)).toBeInTheDocument(),
    )
  })

  it('confirms before deleting a scheduled scan and surfaces its failure', async () => {
    mockApi.get.mockImplementation(pageGet())
    mockApi.del.mockRejectedValue(new Error('scan is running'))
    const confirmSpy = vi.spyOn(window, 'confirm').mockReturnValue(true)

    render(<SettingsPage />)
    await userEvent.click(
      await screen.findByRole('button', { name: /Delete scheduled scan for https:\/\/api\.example\.com\/openapi\.json/ }, { timeout: 5000 }),
    )

    expect(confirmSpy).toHaveBeenCalledOnce()
    await waitFor(() =>
      expect(screen.getByText(/Failed to delete scan: scan is running/)).toBeInTheDocument(),
    )
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

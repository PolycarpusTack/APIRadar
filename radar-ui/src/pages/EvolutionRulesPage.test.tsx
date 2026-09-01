// @vitest-environment jsdom
//
// N-25 regression guard: toggle/delete swallowed their failures with
// `.catch(() => {})`, so the button simply re-enabled itself and the user was
// left believing the rule had changed.
import '@testing-library/jest-dom/vitest'
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import { render, screen, cleanup, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { MemoryRouter } from 'react-router-dom'
import EvolutionRulesPage from './EvolutionRulesPage'

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

const RULE = {
  id: 'rule-1',
  name: 'Allow optional headers',
  change_kind: 'field_added',
  path_pattern: null,
  severity_override: 'safe',
  enabled: true,
  created_at: '2026-06-01T00:00:00Z',
}

function renderPage() {
  return render(<MemoryRouter><EvolutionRulesPage /></MemoryRouter>)
}

beforeEach(() => {
  for (const fn of Object.values(mockApi)) fn.mockReset()
  mockApi.get.mockResolvedValue({ entries: [RULE] })
  localStorage.clear()
})
afterEach(() => {
  cleanup()
  vi.restoreAllMocks()
})

describe('EvolutionRulesPage destructive-action safety', () => {
  it('asks for confirmation and does not delete when it is dismissed', async () => {
    const confirmSpy = vi.spyOn(window, 'confirm').mockReturnValue(false)
    renderPage()

    await userEvent.click(await screen.findByRole('button', { name: /Delete rule/ }, { timeout: 5000 }))

    expect(confirmSpy).toHaveBeenCalledOnce()
    expect(mockApi.del).not.toHaveBeenCalled()
  })

  it('surfaces a delete failure instead of silently re-enabling the button', async () => {
    vi.spyOn(window, 'confirm').mockReturnValue(true)
    mockApi.del.mockRejectedValue(new Error('rule is referenced'))
    renderPage()

    await userEvent.click(await screen.findByRole('button', { name: /Delete rule/ }, { timeout: 5000 }))

    await waitFor(() =>
      expect(screen.getByText(/Failed to delete rule: rule is referenced/)).toBeInTheDocument(),
    )
  })

  it('surfaces a toggle failure', async () => {
    mockApi.patch.mockRejectedValue(new Error('read-only mode'))
    renderPage()

    await userEvent.click(await screen.findByRole('button', { name: 'enabled' }, { timeout: 5000 }))

    await waitFor(() =>
      expect(screen.getByText(/Failed to update rule: read-only mode/)).toBeInTheDocument(),
    )
  })

  it('clears a previous action error once an action succeeds', async () => {
    vi.spyOn(window, 'confirm').mockReturnValue(true)
    mockApi.patch.mockRejectedValueOnce(new Error('read-only mode')).mockResolvedValue({})
    renderPage()

    const toggle = await screen.findByRole('button', { name: 'enabled' }, { timeout: 5000 })
    await userEvent.click(toggle)
    await waitFor(() => expect(screen.getByText(/read-only mode/)).toBeInTheDocument())

    await userEvent.click(screen.getByRole('button', { name: 'enabled' }))
    await waitFor(() => expect(screen.queryByText(/read-only mode/)).not.toBeInTheDocument())
  })
})

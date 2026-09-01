// @vitest-environment jsdom
import { describe, it, expect, beforeEach } from 'vitest'
import {
  SANDBOX_ENVS_LOCAL_KEY,
  loadLocalEnvs,
  saveLocalEnvs,
  type SandboxEnv,
} from './sandboxEnvStorage'

function makeEnv(overrides: Partial<SandboxEnv> = {}): SandboxEnv {
  return {
    id: 'env-1',
    name: 'Prod sandbox',
    base_url: 'https://sandbox.example.com/api',
    bearer_token: 'sk-super-secret-token',
    description: 'demo tenant',
    created_at: '2026-09-01T00:00:00.000Z',
    updated_at: '2026-09-01T00:00:00.000Z',
    ...overrides,
  }
}

function rawStored(): SandboxEnv[] {
  return JSON.parse(localStorage.getItem(SANDBOX_ENVS_LOCAL_KEY) ?? '[]') as SandboxEnv[]
}

beforeEach(() => {
  localStorage.clear()
})

describe('saveLocalEnvs', () => {
  it('never writes bearer_token values to localStorage', () => {
    saveLocalEnvs([makeEnv(), makeEnv({ id: 'env-2', bearer_token: 'another-secret' })])

    const stored = localStorage.getItem(SANDBOX_ENVS_LOCAL_KEY) ?? ''
    expect(stored).not.toContain('sk-super-secret-token')
    expect(stored).not.toContain('another-secret')
    for (const entry of rawStored()) {
      expect(entry.bearer_token).toBe('')
    }
  })

  it('persists every non-secret field unchanged', () => {
    const env = makeEnv()
    saveLocalEnvs([env])

    const [entry] = rawStored()
    expect(entry).toEqual({ ...env, bearer_token: '' })
  })

  it('does not mutate the in-memory envs, so the token stays usable this session', () => {
    const env = makeEnv()
    const envs = [env]
    saveLocalEnvs(envs)

    expect(env.bearer_token).toBe('sk-super-secret-token')
    expect(envs[0].bearer_token).toBe('sk-super-secret-token')
  })
})

describe('loadLocalEnvs', () => {
  it('purges legacy plaintext tokens: strips them and rewrites the cleaned list', () => {
    // A legacy entry written by the old code, token included.
    localStorage.setItem(
      SANDBOX_ENVS_LOCAL_KEY,
      JSON.stringify([makeEnv(), makeEnv({ id: 'env-2', bearer_token: '' })]),
    )

    const loaded = loadLocalEnvs()

    // Returned list is cleaned…
    expect(loaded).toHaveLength(2)
    for (const entry of loaded) expect(entry.bearer_token).toBe('')
    // …and the stored value was rewritten without the token.
    const stored = localStorage.getItem(SANDBOX_ENVS_LOCAL_KEY) ?? ''
    expect(stored).not.toContain('sk-super-secret-token')
    for (const entry of rawStored()) expect(entry.bearer_token).toBe('')
  })

  it('round-trips a clean list untouched', () => {
    const env = makeEnv({ bearer_token: '' })
    saveLocalEnvs([env])
    const before = localStorage.getItem(SANDBOX_ENVS_LOCAL_KEY)

    expect(loadLocalEnvs()).toEqual([env])
    expect(localStorage.getItem(SANDBOX_ENVS_LOCAL_KEY)).toBe(before)
  })

  it('returns [] for a missing key or corrupt JSON', () => {
    expect(loadLocalEnvs()).toEqual([])
    localStorage.setItem(SANDBOX_ENVS_LOCAL_KEY, 'not-json{')
    expect(loadLocalEnvs()).toEqual([])
  })
})

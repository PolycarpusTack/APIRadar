import { describe, it, expect } from 'vitest'
import { extractVariables } from './variableExtractor'
import type { PlaygroundRequest } from './variableExtractor'

function req(overrides: Partial<PlaygroundRequest> = {}): PlaygroundRequest {
  return {
    url: '',
    method: 'GET',
    headers: [],
    body: '',
    ...overrides,
  }
}

describe('extractVariables', () => {
  it('returns empty array when no placeholders', () => {
    expect(extractVariables(req({ url: 'https://api.example.com/users' }))).toEqual([])
  })

  it('extracts variable from URL', () => {
    const vars = extractVariables(req({ url: 'https://api.example.com/{{userId}}/profile' }))
    expect(vars).toContain('userId')
  })

  it('extracts variable from body', () => {
    const vars = extractVariables(req({ body: '{"id": "{{orderId}}"}' }))
    expect(vars).toContain('orderId')
  })

  it('extracts variables from header keys and values', () => {
    const vars = extractVariables(
      req({ headers: [{ key: '{{headerName}}', value: '{{token}}' }] }),
    )
    expect(vars).toContain('headerName')
    expect(vars).toContain('token')
  })

  it('deduplicates variables appearing in multiple places', () => {
    const vars = extractVariables(
      req({
        url: 'https://api.example.com/{{env}}/items',
        body: '{"env": "{{env}}"}',
      }),
    )
    const occurrences = vars.filter(v => v === 'env').length
    expect(occurrences).toBe(1)
  })

  it('extracts multiple distinct variables', () => {
    const vars = extractVariables(
      req({
        url: '{{host}}/{{path}}',
        headers: [{ key: 'Authorization', value: 'Bearer {{token}}' }],
      }),
    )
    expect(vars.sort()).toEqual(['host', 'path', 'token'].sort())
  })

  it('does not match invalid variable names (starts with digit)', () => {
    const vars = extractVariables(req({ url: 'https://example.com/{{1invalid}}' }))
    expect(vars).toHaveLength(0)
  })
})

import { describe, it, expect } from 'vitest'
import { resolveRequest, maskSecrets } from './variableResolver'
import type { PlaygroundRequest } from './variableExtractor'

function tmpl(overrides: Partial<PlaygroundRequest> = {}): PlaygroundRequest {
  return { url: '', method: 'GET', headers: [], body: '', ...overrides }
}

describe('resolveRequest', () => {
  it('resolves a URL placeholder', () => {
    const result = resolveRequest(tmpl({ url: 'https://{{host}}/path' }), { host: 'api.example.com' })
    expect(result.url).toBe('https://api.example.com/path')
    expect(result.unresolved).toHaveLength(0)
  })

  it('resolves a body placeholder', () => {
    const result = resolveRequest(tmpl({ body: '{"id":"{{orderId}}"}' }), { orderId: '42' })
    expect(result.body).toBe('{"id":"42"}')
  })

  it('resolves header key and value placeholders', () => {
    const result = resolveRequest(
      tmpl({ headers: [{ key: 'X-{{name}}', value: '{{val}}' }] }),
      { name: 'Custom', val: 'abc' },
    )
    expect(result.headers['X-Custom']).toBe('abc')
  })

  it('tracks unresolved variables', () => {
    const result = resolveRequest(tmpl({ url: 'https://{{host}}/{{path}}' }), { host: 'api.example.com' })
    expect(result.unresolved).toContain('path')
    expect(result.unresolved).not.toContain('host')
    // Unresolved placeholder remains in place
    expect(result.url).toContain('{{path}}')
  })

  it('passes method through unchanged', () => {
    const result = resolveRequest(tmpl({ method: 'POST' }), {})
    expect(result.method).toBe('POST')
  })

  it('handles multiple placeholders in one string', () => {
    const result = resolveRequest(
      tmpl({ url: '{{scheme}}://{{host}}/{{resource}}' }),
      { scheme: 'https', host: 'example.com', resource: 'items' },
    )
    expect(result.url).toBe('https://example.com/items')
    expect(result.unresolved).toHaveLength(0)
  })
})

describe('maskSecrets', () => {
  it('masks Authorization header value', () => {
    const masked = maskSecrets({ Authorization: 'Bearer abc123' })
    expect(masked['Authorization']).toBe('****')
  })

  it('masks token header (case-insensitive match)', () => {
    const masked = maskSecrets({ 'X-Auth-Token': 'secret' })
    expect(masked['X-Auth-Token']).toBe('****')
  })

  it('masks key header', () => {
    const masked = maskSecrets({ 'x-api-key': 'mykey' })
    expect(masked['x-api-key']).toBe('****')
  })

  it('masks password header', () => {
    const masked = maskSecrets({ 'X-Password': 'hunter2' })
    expect(masked['X-Password']).toBe('****')
  })

  it('masks bearer header', () => {
    const masked = maskSecrets({ 'X-Bearer': 'tok' })
    expect(masked['X-Bearer']).toBe('****')
  })

  it('does not mask non-sensitive headers', () => {
    const masked = maskSecrets({ 'Content-Type': 'application/json', 'Accept': '*/*' })
    expect(masked['Content-Type']).toBe('application/json')
    expect(masked['Accept']).toBe('*/*')
  })

  it('returns empty object for empty input', () => {
    expect(maskSecrets({})).toEqual({})
  })
})

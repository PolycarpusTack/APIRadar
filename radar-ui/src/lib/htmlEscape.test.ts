import { describe, it, expect } from 'vitest'
import { escapeHtmlAttr, escapeJsonForHtml } from './htmlEscape'

const XSS = `'"><script>alert(1)</script>`

describe('escapeHtmlAttr', () => {
  it('neutralizes an XSS payload so it cannot break out of an attribute', () => {
    const escaped = escapeHtmlAttr(XSS)
    // No raw tag opener/closer and no raw quotes that would end the attribute
    expect(escaped).not.toContain('<')
    expect(escaped).not.toContain('>')
    expect(escaped).not.toContain('"')
    expect(escaped).not.toContain("'")
    expect(escaped).not.toContain('<script')
    // Characters are HTML-entity encoded instead
    expect(escaped).toContain('&lt;')
    expect(escaped).toContain('&gt;')
  })

  it('escapes ampersands first so entities are not double-broken', () => {
    expect(escapeHtmlAttr('a & b')).toBe('a &amp; b')
  })

  it('leaves safe values untouched', () => {
    expect(escapeHtmlAttr('https://api.example.com/openapi.yaml')).toBe(
      'https://api.example.com/openapi.yaml',
    )
  })
})

describe('escapeJsonForHtml', () => {
  it('embeds an XSS payload without any raw angle brackets or quotes', () => {
    const blob = escapeJsonForHtml({ servers: [{ url: XSS }] })
    expect(blob).not.toContain('<')
    expect(blob).not.toContain('>')
    expect(blob).not.toContain('<script')
    // The single quote used as an attribute delimiter must be escaped too
    expect(blob).not.toContain("'")
  })

  it('stays valid, round-trippable JSON', () => {
    const value = { servers: [{ url: XSS }], token: `a'b"c` }
    const parsed = JSON.parse(escapeJsonForHtml(value))
    expect(parsed).toEqual(value)
  })
})

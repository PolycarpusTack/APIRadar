import { describe, it, expect } from 'vitest'
import { isSharePath, stripBasename } from './sharePath'

describe('stripBasename', () => {
  it('strips the production /app basename', () => {
    expect(stripBasename('/app/share/abc', '/app')).toBe('/share/abc')
    expect(stripBasename('/app/diffs/1', '/app/')).toBe('/diffs/1')
  })

  it('returns the bare basename as root', () => {
    expect(stripBasename('/app', '/app')).toBe('/')
  })

  it('leaves the path unchanged when there is no basename', () => {
    expect(stripBasename('/share/abc', '')).toBe('/share/abc')
  })

  it('does not strip a look-alike prefix that is not a path segment', () => {
    expect(stripBasename('/application/share/abc', '/app')).toBe('/application/share/abc')
  })
})

describe('isSharePath', () => {
  it('matches the dev share route (no basename)', () => {
    expect(isSharePath('/share/tok123', '')).toBe(true)
  })

  it('matches the production share route under /app', () => {
    expect(isSharePath('/app/share/tok123', '/app')).toBe(true)
  })

  it('does not match non-share routes under /app', () => {
    expect(isSharePath('/app/diffs/1', '/app')).toBe(false)
    expect(isSharePath('/app', '/app')).toBe(false)
  })

  it('does not match a path that merely contains the word share', () => {
    expect(isSharePath('/app/shared-links', '/app')).toBe(false)
  })
})

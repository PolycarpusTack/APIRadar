import { describe, it, expect } from 'vitest'
import { parseCsv } from './csvParser'

describe('parseCsv', () => {
  it('returns error for empty string', () => {
    const r = parseCsv('')
    expect(r.error).toBe('CSV file is empty')
    expect(r.rows).toHaveLength(0)
  })

  it('returns error for whitespace-only input', () => {
    const r = parseCsv('   \n  ')
    expect(r.error).toBe('CSV file is empty')
  })

  it('parses a simple header-only CSV', () => {
    const r = parseCsv('name,age')
    expect(r.headers).toEqual(['name', 'age'])
    expect(r.rows).toHaveLength(0)
    expect(r.error).toBeUndefined()
  })

  it('parses basic rows', () => {
    const r = parseCsv('name,age\nAlice,30\nBob,25')
    expect(r.headers).toEqual(['name', 'age'])
    expect(r.rows).toHaveLength(2)
    expect(r.rows[0]).toEqual({ name: 'Alice', age: '30' })
    expect(r.rows[1]).toEqual({ name: 'Bob', age: '25' })
  })

  it('handles CRLF line endings', () => {
    const r = parseCsv('a,b\r\n1,2\r\n3,4')
    expect(r.rows).toHaveLength(2)
    expect(r.rows[0]).toEqual({ a: '1', b: '2' })
  })

  it('strips UTF-8 BOM', () => {
    const r = parseCsv('﻿col\nval')
    expect(r.headers).toEqual(['col'])
    expect(r.rows[0]).toEqual({ col: 'val' })
  })

  it('parses quoted fields containing commas', () => {
    const r = parseCsv('a,b\n"hello, world",plain')
    expect(r.rows[0]).toEqual({ a: 'hello, world', b: 'plain' })
  })

  it('handles escaped double-quotes inside quoted fields', () => {
    const r = parseCsv('q\n"say ""hi"""')
    expect(r.rows[0]).toEqual({ q: 'say "hi"' })
  })

  it('returns error for empty column header', () => {
    const r = parseCsv('name,,age')
    expect(r.error).toBe('CSV contains an empty column header')
  })

  it('returns error for duplicate column header', () => {
    const r = parseCsv('name,name,age')
    expect(r.error).toMatch(/Duplicate column header/)
  })

  it('fills missing fields with empty string', () => {
    const r = parseCsv('a,b,c\n1,2')
    expect(r.rows[0]).toEqual({ a: '1', b: '2', c: '' })
  })

  it('enforces row limit', () => {
    const rows = Array.from({ length: 5 }, (_, i) => `${i},${i}`).join('\n')
    const r = parseCsv(`a,b\n${rows}`, 3)
    expect(r.error).toMatch(/row limit/)
    expect(r.rows).toHaveLength(3)
  })

  it('skips blank lines between rows', () => {
    const r = parseCsv('a,b\n1,2\n\n3,4')
    expect(r.rows).toHaveLength(2)
  })
})

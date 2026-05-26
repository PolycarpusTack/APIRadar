import { describe, it, expect } from 'vitest'
import { exportResults, exportFailedRows } from './csvExporter'
import type { RowResult } from './csvExporter'

function row(overrides: Partial<RowResult> = {}): RowResult {
  return {
    rowNumber: 1,
    httpStatus: 200,
    durationMs: 100,
    error: null,
    url: 'https://api.example.com/items',
    originalRow: {},
    ...overrides,
  }
}

async function blobText(blob: Blob): Promise<string> {
  return blob.text()
}

describe('exportResults', () => {
  it('produces correct CSV headers', async () => {
    const blob = exportResults([])
    const text = await blobText(blob)
    expect(text.split('\r\n')[0]).toBe('row_number,http_status,duration_ms,error,url')
  })

  it('encodes a successful row', async () => {
    const blob = exportResults([row()])
    const lines = (await blobText(blob)).split('\r\n')
    expect(lines[1]).toBe('1,200,100,,https://api.example.com/items')
  })

  it('uses empty string for null httpStatus', async () => {
    const blob = exportResults([row({ httpStatus: null })])
    const lines = (await blobText(blob)).split('\r\n')
    expect(lines[1]).toMatch(/^1,,/)
  })

  it('includes error text when present', async () => {
    const blob = exportResults([row({ error: 'timeout', httpStatus: null })])
    const lines = (await blobText(blob)).split('\r\n')
    expect(lines[1]).toContain('timeout')
  })

  it('escapes formula-injection characters with a leading quote', async () => {
    const blob = exportResults([row({ url: '=SUM(A1)' })])
    const lines = (await blobText(blob)).split('\r\n')
    expect(lines[1]).toContain("'=SUM(A1)")
  })

  it('quotes URL values containing commas', async () => {
    const blob = exportResults([row({ url: 'https://ex.com?a=1,b=2' })])
    const lines = (await blobText(blob)).split('\r\n')
    expect(lines[1]).toContain('"https://ex.com?a=1,b=2"')
  })
})

describe('exportFailedRows', () => {
  it('returns sentinel blob when there are no failures', async () => {
    const blob = exportFailedRows([row({ httpStatus: 200, error: null })])
    const text = await blobText(blob)
    expect(text).toBe('No failed rows')
  })

  it('includes 4xx rows', async () => {
    const failed = row({ httpStatus: 404, error: null, originalRow: { id: '1' } })
    const blob = exportFailedRows([failed])
    const text = await blobText(blob)
    expect(text).toContain('404')
  })

  it('includes 5xx rows', async () => {
    const failed = row({ httpStatus: 500, error: null, originalRow: { id: '2' } })
    const blob = exportFailedRows([failed])
    const text = await blobText(blob)
    expect(text).toContain('500')
  })

  it('includes rows with fetch errors regardless of httpStatus', async () => {
    const failed = row({ httpStatus: null, error: 'network error', originalRow: { id: '3' } })
    const blob = exportFailedRows([failed])
    const text = await blobText(blob)
    expect(text).toContain('network error')
  })

  it('excludes successful rows', async () => {
    const ok = row({ rowNumber: 1, httpStatus: 200, error: null, originalRow: { id: 'ok' } })
    const bad = row({ rowNumber: 2, httpStatus: 422, error: null, originalRow: { id: 'bad' } })
    const blob = exportFailedRows([ok, bad])
    const text = await blobText(blob)
    // The failed row's original data must appear; the OK row's must not
    const lines = text.split('\r\n')
    const dataLines = lines.slice(1)
    expect(dataLines.some(l => l.includes('bad'))).toBe(true)
    expect(dataLines.some(l => l.includes('ok'))).toBe(false)
  })
})

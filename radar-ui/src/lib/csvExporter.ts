export interface RowResult {
  rowNumber: number
  httpStatus: number | null
  durationMs: number
  error: string | null
  url: string
  originalRow: Record<string, string>
}

const FORMULA_PREFIXES = /^[=+\-@]/

/** Escape a CSV cell value: quote if contains comma/newline/quote, escape formula injection. */
function escapeCell(value: string): string {
  // Formula injection protection
  if (FORMULA_PREFIXES.test(value)) {
    value = "'" + value
  }
  // Quote if necessary
  if (value.includes(',') || value.includes('"') || value.includes('\n') || value.includes('\r')) {
    value = '"' + value.replace(/"/g, '""') + '"'
  }
  return value
}

function buildCsv(headers: string[], dataRows: string[][]): Blob {
  const lines = [
    headers.map(escapeCell).join(','),
    ...dataRows.map(row => row.map(escapeCell).join(',')),
  ]
  return new Blob([lines.join('\r\n')], { type: 'text/csv;charset=utf-8;' })
}

/** Export all run results as a CSV file. */
export function exportResults(results: RowResult[]): Blob {
  const headers = ['row_number', 'http_status', 'duration_ms', 'error', 'url']
  const rows = results.map(r => [
    String(r.rowNumber),
    r.httpStatus != null ? String(r.httpStatus) : '',
    String(r.durationMs),
    r.error ?? '',
    r.url,
  ])
  return buildCsv(headers, rows)
}

/** Export only the original input rows that produced errors (HTTP 4xx/5xx or fetch error). */
export function exportFailedRows(results: RowResult[]): Blob {
  const failed = results.filter(r => r.error != null || (r.httpStatus != null && r.httpStatus >= 400))
  if (failed.length === 0) return new Blob(['No failed rows'], { type: 'text/csv;charset=utf-8;' })

  const inputHeaders = failed.length > 0 ? Object.keys(failed[0].originalRow) : []
  const extraHeaders = ['error', 'http_status']
  const headers = [...inputHeaders, ...extraHeaders]

  const rows = failed.map(r => [
    ...inputHeaders.map(h => r.originalRow[h] ?? ''),
    r.error ?? '',
    r.httpStatus != null ? String(r.httpStatus) : '',
  ])
  return buildCsv(headers, rows)
}

export function triggerDownload(blob: Blob, filename: string) {
  const url = URL.createObjectURL(blob)
  const a = document.createElement('a')
  a.href = url
  a.download = filename
  a.click()
  URL.revokeObjectURL(url)
}

export interface ParseResult {
  headers: string[]
  rows: Record<string, string>[]
  error?: string
}

const MAX_ROWS = 500
const MAX_BYTES = 10 * 1024 * 1024 // 10 MB

/** Parse a single CSV field, handling RFC 4180 quoting. */
function parseField(text: string, pos: number): [value: string, next: number] {
  if (text[pos] !== '"') {
    // Unquoted field — read until comma or newline
    const start = pos
    while (pos < text.length && text[pos] !== ',' && text[pos] !== '\n' && text[pos] !== '\r') {
      pos++
    }
    return [text.slice(start, pos), pos]
  }

  // Quoted field
  pos++ // skip opening quote
  let value = ''
  while (pos < text.length) {
    if (text[pos] === '"') {
      if (text[pos + 1] === '"') {
        // Escaped quote
        value += '"'
        pos += 2
      } else {
        pos++ // skip closing quote
        break
      }
    } else {
      value += text[pos]
      pos++
    }
  }
  return [value, pos]
}

/** Parse a single CSV line, returning [fields, nextLineStart]. */
function parseLine(text: string, pos: number): [fields: string[], next: number] {
  const fields: string[] = []
  while (pos <= text.length) {
    const [value, next] = parseField(text, pos)
    fields.push(value)
    pos = next
    if (pos >= text.length || text[pos] === '\n' || text[pos] === '\r') {
      // Skip \r\n or \n
      if (text[pos] === '\r') pos++
      if (text[pos] === '\n') pos++
      break
    }
    if (text[pos] === ',') {
      pos++ // skip comma
    }
  }
  return [fields, pos]
}

export function parseCsv(text: string, maxRows = MAX_ROWS): ParseResult {
  if (new TextEncoder().encode(text).length > MAX_BYTES) {
    return { headers: [], rows: [], error: 'File exceeds 10 MB limit' }
  }

  // Strip UTF-8 BOM
  let src = text.startsWith('﻿') ? text.slice(1) : text
  src = src.trim()

  if (!src) {
    return { headers: [], rows: [], error: 'CSV file is empty' }
  }

  let pos = 0

  // Parse header row
  const [headers, next] = parseLine(src, pos)
  pos = next

  // Validate: no empty headers, no duplicates
  for (const h of headers) {
    if (!h.trim()) {
      return { headers: [], rows: [], error: 'CSV contains an empty column header' }
    }
  }
  const seen = new Set<string>()
  for (const h of headers) {
    if (seen.has(h)) {
      return { headers: [], rows: [], error: `Duplicate column header: ${h}` }
    }
    seen.add(h)
  }

  const rows: Record<string, string>[] = []

  while (pos < src.length) {
    if (rows.length >= maxRows) {
      return { headers, rows, error: `CSV exceeds ${maxRows} row limit (excluding header)` }
    }
    const [fields, next] = parseLine(src, pos)
    pos = next

    // Skip blank lines
    if (fields.length === 1 && fields[0] === '') continue

    if (fields.length > headers.length) {
      return {
        headers,
        rows,
        error: `Row ${rows.length + 1} has ${fields.length} columns but the header has ${headers.length} — possible shifted column`,
      }
    }

    const row: Record<string, string> = {}
    headers.forEach((h, i) => {
      row[h] = fields[i] ?? ''
    })
    rows.push(row)
  }

  return { headers, rows }
}

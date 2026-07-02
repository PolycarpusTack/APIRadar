// HTML escaping helpers for safely embedding untrusted values into raw HTML
// strings (e.g. an iframe `srcDoc`). Sandbox-environment values are shared
// server-side, so a malicious value must never break out of its HTML context.

const HTML_ATTR_REPLACEMENTS: Record<string, string> = {
  '&': '&amp;',
  '<': '&lt;',
  '>': '&gt;',
  '"': '&quot;',
  "'": '&#39;',
}

/**
 * Escape a string for use inside an HTML attribute value (works for both
 * single- and double-quoted attributes). Neutralizes `& < > " '` so the value
 * cannot close the attribute or open a new tag.
 */
export function escapeHtmlAttr(value: string): string {
  return value.replace(/[&<>"']/g, (ch) => HTML_ATTR_REPLACEMENTS[ch])
}

// Characters that are dangerous when JSON is embedded in HTML: the tag/entity
// characters, the single quote (attribute delimiter), and the U+2028/U+2029
// line separators (referenced via their code points so the regex source stays
// single-line — a raw separator would terminate the literal). The double quote
// is deliberately excluded: JSON.stringify uses it as a structural delimiter,
// and the blob is embedded in a single-quoted HTML attribute where `"` is safe.
const JSON_HTML_UNSAFE = new RegExp("[<>&'\\u2028\\u2029]", 'g')

/**
 * Serialize a value to JSON and neutralize the characters that are dangerous in
 * an HTML context, using `\uXXXX` escapes so the result stays valid JSON.
 *
 * Escapes `<`, `>`, `&` (prevents `</script>` / tag injection when the blob is
 * placed in a `<script>` body or attribute) and the single quote so the blob
 * can be embedded in a single-quoted HTML attribute without breaking out.
 */
export function escapeJsonForHtml(value: unknown): string {
  return JSON.stringify(value).replace(
    JSON_HTML_UNSAFE,
    (ch) => '\\u' + ch.charCodeAt(0).toString(16).padStart(4, '0'),
  )
}

// Accessibility helpers shared across pages.

import type { KeyboardEvent } from 'react'

/**
 * onKeyDown handler that makes a non-button element (e.g. a clickable table
 * `<tr>`) behave like a button for keyboard users: Enter or Space activates it.
 * Pair with `role="button"` and `tabIndex={0}` on the same element.
 */
export function activateOnKey(handler: () => void) {
  return (e: KeyboardEvent) => {
    if (e.key === 'Enter' || e.key === ' ' || e.key === 'Spacebar') {
      e.preventDefault()
      handler()
    }
  }
}

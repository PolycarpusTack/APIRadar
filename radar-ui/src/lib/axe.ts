// axe-core harness for the accessibility smoke tests (N-22-T1).
//
// The per-page RTL tests assert specific, named remediations ("the close button
// is called X"). This harness is the complementary net: it runs the full axe
// ruleset over a rendered tree so a *new* violation — an input that loses its
// label, a button that becomes icon-only — fails CI without anyone having
// thought to write an assertion for it first.

import axe from 'axe-core'

// jsdom has no layout engine and no CSS cascade, so a handful of axe rules
// cannot produce a meaningful verdict there. Disabling them keeps the harness
// honest: a rule that cannot run should not report a pass either.
//
//   color-contrast     — needs computed pixel colours; the app's palette is
//                        AIR design tokens, which jsdom never resolves.
//   region             — "all content sits in a landmark" is a whole-document
//                        rule; these tests render one page/panel in isolation,
//                        below the App shell that provides <main>.
const JSDOM_UNRUNNABLE_RULES: axe.RuleObject = {
  'color-contrast': { enabled: false },
  region: { enabled: false },
}

/** One violation, flattened to the fields worth printing in a test failure. */
export interface A11yViolation {
  id: string
  impact: string
  help: string
  nodes: string[]
}

/**
 * Run axe over `container` and return the violations found.
 *
 * @param container Root element to scan — normally the `container` returned by
 *                  RTL's `render()`.
 * @param rules     Extra per-rule overrides, merged over the jsdom defaults.
 *                  Use this to disable a rule that a specific fixture cannot
 *                  satisfy, and say why at the call site.
 */
export async function findA11yViolations(
  container: HTMLElement,
  rules: axe.RuleObject = {},
): Promise<A11yViolation[]> {
  const results = await axe.run(container, {
    rules: { ...JSDOM_UNRUNNABLE_RULES, ...rules },
    // axe scans nested frames by messaging into them, which jsdom cannot do
    // ("Respondable target must be a frame in the current window" — the
    // Playground's sandboxed Scalar iframe hits this). Frame content is
    // third-party and out of scope for these tests anyway.
    iframes: false,
    // Report only genuine failures; "incomplete" results are cases axe could
    // not decide in jsdom and would be noise here.
    resultTypes: ['violations'],
  })

  return results.violations.map((v) => ({
    id: v.id,
    impact: v.impact ?? 'unknown',
    help: v.help,
    nodes: v.nodes.map((n) => n.html),
  }))
}

/** Human-readable report for a failed assertion. */
export function formatViolations(violations: A11yViolation[]): string {
  return violations
    .map(
      (v) =>
        `  [${v.impact}] ${v.id}: ${v.help}\n` +
        v.nodes.map((n) => `      ${n.slice(0, 200)}`).join('\n'),
    )
    .join('\n')
}

import { useState } from 'react'
import { HelpCircle } from 'lucide-react'

// ---------------------------------------------------------------------------
// Term definitions
// ---------------------------------------------------------------------------

export const TERM_DEFINITIONS: Record<string, string> = {
  blast_radius:
    'The set of consumers affected by one or more breaking changes — each with a confidence level based on how recently and how definitively they were observed calling the changed field.',
  confidence_high:
    'High confidence: the consumer sent live traffic to this operation within the last 7 days, confirmed by runtime telemetry (OTel traces or gateway logs).',
  confidence_medium:
    'Medium confidence: the consumer was seen using this field in a static code scan or Postman collection, but no live traffic was observed recently.',
  confidence_low:
    'Low confidence: a code scan found a possible reference to this field, but the operation could not be identified. Treat as a signal, not a certainty.',
  evidence:
    'Proof that a consumer uses a specific API field. Evidence comes from three sources: runtime telemetry (OTel traces), static code scans (tree-sitter), and Postman collection files.',
  evidence_runtime_usage:
    'Runtime evidence: the consumer was observed calling this field in live traffic via OTel traces or gateway logs. Highest confidence.',
  evidence_static_call_site:
    "Static call-site evidence: the consumer's source code references this field, found by a tree-sitter scan of the repo. Medium confidence.",
  evidence_collection_file:
    'Collection-file evidence: a Postman (or NativeREST) collection owned by the consumer contains a request that uses this field. Medium confidence.',
  fail_mode:
    'Controls what happens when the Radar API is unreachable during a drift check. "closed" = block the build; "open" = use local diff only and warn; "warn" = never block.',
  lookback_window:
    'How far back Radar looks for usage evidence when computing blast radius. Evidence older than this window is ignored for policy decisions.',
  change_kind_field_removed:    'field_removed — a response or request body field was deleted. Any consumer reading that field will break.',
  change_kind_field_added:      'field_added — a new field was added to the response or request body. Usually safe, but may require consumers to handle the new field.',
  change_kind_type_changed:     'type_changed — the type of an existing field changed (e.g. string → integer). Will break deserialisation in consumers.',
  change_kind_required_changed: "required_changed — a field's required status changed. Adding required to a request field breaks consumers that omit it.",
  change_kind_operation_removed:'operation_removed — an entire endpoint was removed. Any consumer calling it will get 404.',
  change_kind_operation_added:  'operation_added — a new endpoint was added. Safe for existing consumers.',
  change_kind_parameter_removed:'parameter_removed — a path, query, or header parameter was removed.',
  change_kind_response_removed: 'response_removed — a documented HTTP response code was removed from the spec.',
  change_kind_enum_value_removed:'enum_value_removed — a valid enum value was deleted. Consumers that send or match this value will break.',
  change_kind_enum_value_added: 'enum_value_added — a new enum value was added. Safe for consumers, but they may not handle unknown values.',
  change_kind_nullability_changed: "nullability_changed — a field's nullability changed (nullable ↔ non-nullable).",
  change_kind_request_body_added:'request_body_added — a request body was added to an operation that previously had none.',
  change_kind_request_body_removed:'request_body_removed — a required request body was removed from an operation.',
  policy_decision:
    'A recorded verdict (PASS / WARN / BLOCK) from a drift check run. Stores which changes triggered the verdict and whether an override was in effect.',
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

interface TermTooltipProps {
  term: keyof typeof TERM_DEFINITIONS
  /** Position the popover above or below the icon (default: above) */
  placement?: 'top' | 'bottom'
}

export default function TermTooltip({ term, placement = 'top' }: TermTooltipProps) {
  const [visible, setVisible] = useState(false)
  const definition = TERM_DEFINITIONS[term]
  if (!definition) return null

  const popoverStyle: React.CSSProperties = {
    position: 'absolute',
    left: '50%',
    transform: 'translateX(-50%)',
    width: 280,
    padding: '8px 10px',
    borderRadius: 6,
    background: 'var(--bg-tooltip, #1a1a2e)',
    border: '1px solid var(--border)',
    color: 'var(--text-1)',
    fontSize: 11.5,
    lineHeight: 1.6,
    zIndex: 50,
    pointerEvents: 'none',
    boxShadow: '0 4px 12px rgba(0,0,0,0.35)',
    ...(placement === 'top' ? { bottom: '100%', marginBottom: 6 } : { top: '100%', marginTop: 6 }),
  }

  return (
    <span
      className="relative inline-flex items-center"
      style={{ verticalAlign: 'middle' }}
      onMouseEnter={() => setVisible(true)}
      onMouseLeave={() => setVisible(false)}
      onFocus={() => setVisible(true)}
      onBlur={() => setVisible(false)}
    >
      <button
        type="button"
        tabIndex={0}
        aria-label={`Definition: ${term.replace(/_/g, ' ')}`}
        className="flex items-center justify-center rounded-full transition-opacity hover:opacity-80 focus:outline-none focus-visible:ring-1"
        style={{ color: 'var(--text-dim)', width: 14, height: 14 }}
      >
        <HelpCircle className="h-3 w-3" />
      </button>

      {visible && (
        <span role="tooltip" style={popoverStyle}>
          {definition}
        </span>
      )}
    </span>
  )
}

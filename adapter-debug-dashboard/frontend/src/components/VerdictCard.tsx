// VerdictCard — renders the decoded ActionEnvelope list and the aggregated
// policy verdict for a single captured RPC event. The parent calls /api/route
// automatically when an event arrives via SSE and stores the result in the
// EventEntry; we just present it.
//
// If `routed === undefined`, the request is still in flight.
// If `routed.actions.length === 0`, the route returned no envelopes (e.g.
//   selector with no mapper registered) — we still show a placeholder card so
//   the user knows something landed.
// If `routed.verdict === undefined`, evaluation failed — we render the decode
//   result only, plus the `policy_error` reason as a banner. (Decision 3b.)

import { useMemo } from 'react'

export interface MatchedPolicySummary {
  policy_id: string
  severity: string
  message?: string | null
}

export type RouteVerdict =
  | { decision: 'pass' }
  | { decision: 'warn'; matched: MatchedPolicySummary[] }
  | { decision: 'fail'; matched: MatchedPolicySummary[] }

export interface RouteResult {
  actions: unknown[]
  verdict?: RouteVerdict
  policy_error?: string
}

export interface VerdictCardProps {
  routed?: RouteResult
  inflight: boolean
}

export function VerdictCard({ routed, inflight }: VerdictCardProps) {
  const verdictBadge = useMemo(() => {
    if (inflight) {
      return <span className="verdict-pill verdict-pill-pending">Evaluating…</span>
    }
    if (!routed) {
      return null
    }
    if (routed.policy_error) {
      return <span className="verdict-pill verdict-pill-error">Policy error</span>
    }
    if (!routed.verdict) {
      return <span className="verdict-pill verdict-pill-unknown">No verdict</span>
    }
    switch (routed.verdict.decision) {
      case 'pass':
        return <span className="verdict-pill verdict-pill-pass">Allow</span>
      case 'warn':
        return <span className="verdict-pill verdict-pill-warn">Warn</span>
      case 'fail':
        return <span className="verdict-pill verdict-pill-fail">Deny</span>
    }
  }, [routed, inflight])

  if (!routed && !inflight) return null

  const matched =
    routed?.verdict && 'matched' in routed.verdict ? routed.verdict.matched : []

  return (
    <div className="verdict-card">
      <div className="verdict-card-header">
        <strong>Policy evaluation</strong>
        {verdictBadge}
      </div>

      {routed?.policy_error && (
        <div className="verdict-card-error">
          policy engine error: {routed.policy_error}
        </div>
      )}

      {matched.length > 0 && (
        <ul className="verdict-card-matches">
          {matched.map((m, i) => (
            <li key={`${m.policy_id}-${i}`} className={`matched matched-${m.severity}`}>
              <code>{m.policy_id}</code>
              {m.message && <span className="matched-reason"> — {m.message}</span>}
            </li>
          ))}
        </ul>
      )}

      {routed && routed.actions.length > 0 && (
        <details className="verdict-card-actions">
          <summary>
            Decoded actions ({routed.actions.length})
          </summary>
          <pre>{JSON.stringify(routed.actions, null, 2)}</pre>
        </details>
      )}

      {routed && routed.actions.length === 0 && !routed.policy_error && (
        <div className="verdict-card-empty">
          No mapper matched this call — decoded structure unavailable.
        </div>
      )}
    </div>
  )
}

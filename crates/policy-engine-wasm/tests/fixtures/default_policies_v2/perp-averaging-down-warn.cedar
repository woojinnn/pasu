// dambi default v2 policy — explorer id: J1 / BEHV (averaging down — re-check
// your thesis before buying more of a losing position). Personal-trading
// (behavioral) family.
//
// Rule: when adding to a position that is already underwater past the user's
// DCA-loss threshold, warn so the user re-confirms the investment thesis before
// piling in (the classic loss-chasing trap).
//
// WHY a method (and why J1 is PART, not COV):
//   the EXISTING position's unrealized PnL % is a State1 value the reducer derives
//   (size × (mark − entry)); it is NOT in the action's live_inputs (those describe
//   the NEW size). For PERP positions this is directly available — the manifest
//   REUSES `perp.position_state`, projecting `unrealizedPnlPct` into the `decimal`
//   `positionPnlPct`. Defined-not-wired → `optional` (fail-open), so the field is
//   `has`-guarded; decimal uses `.lessThan(...)`. The SPOT variant of J1 needs a
//   per-holding `cost_basis` that TokenHolding does not carry yet (the GAP slice)
//   — hence the explorer marks J1 as PART. This fixture ships the perp slice (the
//   behavioral framing of PERP-09's same signal). −15% is user config.
@id("perp-averaging-down-warn")
@severity("warn")
@reason("You're buying more of a position that's already in the red — re-check your thesis before averaging down")
forbid(principal, action == Perp::Action::"IncreasePosition", resource)
when {
  context has custom
  && context.custom has positionPnlPct
  && context.custom.positionPnlPct.lessThan(decimal("-15.0000"))
};

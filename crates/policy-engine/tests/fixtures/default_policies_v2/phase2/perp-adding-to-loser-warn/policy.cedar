// scopeball default v2 policy — explorer id: PERP-09 (averaging down — adding
// size/margin to a position that's already underwater).
//
// Pouring more into a losing position is the #1 way accounts blow up. The signal
// is the EXISTING position's unrealized PnL, a State₁ value the reducer derives
// from size_base × (mark − entry) (recompute_perp_pnl) — it is NOT in the action's
// live_inputs (those describe the new size). The manifest reuses
// `perp.position_state`, projecting `unrealizedPnlPct` into the `decimal`
// `positionUnrealizedPnlPct`. An IncreasePosition is by construction same-side
// add, so this slice keys purely on PnL depth. Defined-not-wired → `optional`
// (fail-open). Warn when the position is worse than −20% (user config; negative
// PnL → the value is below the threshold).
@id("perp-adding-to-loser-warn")
@severity("warn")
@reason("You're adding to a position that's already deep in the red — this is how accounts get wiped")
forbid(principal, action == Perp::Action::"IncreasePosition", resource)
when {
  context has custom
  && context.custom has positionUnrealizedPnlPct
  && context.custom.positionUnrealizedPnlPct.lessThan(decimal("-20.0000"))
};

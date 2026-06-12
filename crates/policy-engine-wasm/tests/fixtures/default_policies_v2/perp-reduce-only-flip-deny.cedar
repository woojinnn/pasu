// dambi default v2 policy — explorer id: PERP-08 (a "reduce" order that
// overshoots zero and flips into a position on the other side).
//
// Mis-sizing a close/reduce can sail past 0 and open a fresh OPPOSITE position —
// Hyperliquid's infamous "reduce" that became a $100M short. Detecting the
// overshoot needs the CURRENT position's size from wallet state
// (`PerpPosition.size_base`, State₁) compared to the order's decrease size; that
// size is a U256-hex String (not Cedar-comparable) and the current position is
// not in the action context. The manifest's `perp.crosses_zero` does the
// comparison and emits the `Bool` `crossesZero`. Defined-not-wired → `optional`
// (fail-open). Deny outright — a flip is never the intent of a reduce.
@id("perp-reduce-only-flip-deny")
@severity("deny")
@reason("This reduce order is larger than your position — it would flip you to the opposite side")
forbid(principal, action == Perp::Action::"DecreasePosition", resource)
when {
  context has custom
  && context.custom has crossesZero
  && context.custom.crossesZero
};

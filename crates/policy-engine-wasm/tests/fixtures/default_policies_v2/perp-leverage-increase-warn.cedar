// dambi default v2 policy — explorer id: PERP-11 (raising the leverage on a
// position you already hold).
//
// Bumping leverage on an open position (especially isolated) pulls margin out and
// walks liq_price toward mark — the same move now liquidates sooner. The trigger
// is "a position exists in this market AND newLeverage > current leverage". Both
// the existence check (`PerpPosition.size_base ≠ 0`, State₁) and the current
// leverage (`PerpPosition.leverage`, decimal, State₁) live in wallet state, while
// `context.newLeverage` is a `Decimal`-as-String the action carries — Cedar can't
// compare a String to a state decimal. The manifest's `perp.leverage_increase`
// resolves the position and does the comparison, emitting the `Bool`
// `leverageIncrease`. Defined-not-wired → `optional` (fail-open).
@id("perp-leverage-increase-warn")
@severity("warn")
@reason("Raising leverage on this open position moves its liquidation price closer to the current price")
forbid(principal, action == Perp::Action::"ChangeLeverage", resource)
when {
  context has custom
  && context.custom has leverageIncrease
  && context.custom.leverageIncrease
};

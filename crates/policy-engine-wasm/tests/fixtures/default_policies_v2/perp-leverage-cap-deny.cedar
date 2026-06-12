// dambi default v2 policy — explorer id: PERP-01 (leverage above your
// self-set cap).
//
// High leverage thins the distance to liquidation — a brief wick can wipe the
// account (the James Wynn $100M → $900 story). Deny an open whose effective
// leverage exceeds the user's cold-decided cap (10x here, user config).
//
// The order's leverage CANNOT be read in Cedar from the base context: both
// `context.leverage` and `context.size.leverage` (the `leverage_implied`
// SizeSpec arm) are raw `Decimal`-as-String fields, and Cedar cannot compare a
// String numerically. The manifest's `perp.order_leverage` call resolves the
// effective leverage (explicit, or collateral×size-implied) into the enriched
// `decimal` `orderLeverage`. Defined-not-implemented → `optional` (fail-open).
@id("perp-leverage-cap-deny")
@severity("deny")
@reason("This entry's leverage is above the cap you set for yourself")
forbid(principal, action == Perp::Action::"OpenPosition", resource)
when {
  context has custom
  && context.custom has orderLeverage
  && context.custom.orderLeverage.greaterThan(decimal("10.0000"))
};

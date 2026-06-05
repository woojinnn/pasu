// scopeball default v2 policy — explorer id: PERP-03 (position concentration).
//
// Rule: warn if this position's notional is large relative to the account's
// total collateral (ratio > 5.0000 = "risking too much per trade"). The ratio
// folds the account's collateral against the order's notional, so the manifest
// declares the STATE fact `perp.notional_to_collateral` served by
// simulation-server. Defined-not-implemented → `optional` (fail-open no-op).
// This fixture proves .cedar + manifest consistency and SPECIFIES the method.
// NOTE (self-verify): on OpenPosition the account collateral is already in the
// action's live_inputs (`userAccountState.totalCollateralUsd`) and notional
// derives from `size × markPrice` — so this is closer to a *derived* read of
// live_inputs than a whole-wallet fold (see coverage note). Limit is user config.
@id("perp-concentration-warn")
@severity("warn")
@reason("This position's notional is large relative to your account collateral")
forbid(principal, action == Perp::Action::"OpenPosition", resource)
when {
  context has custom
  && context.custom has notionalToCollateralRatio
  && context.custom.notionalToCollateralRatio.greaterThan(decimal("5.0000"))
};

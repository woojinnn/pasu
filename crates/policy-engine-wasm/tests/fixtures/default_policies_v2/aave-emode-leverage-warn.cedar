// scopeball default v2 policy — explorer id: AAVE-06 (eMode/collateral toggle
// pushes looping leverage too high).
//
// STATE / REDUCER fact. Bundling correlated assets (e.g. stETH/ETH) into eMode
// raises the LTV cap so the same collateral can borrow far more — looping
// leverage that breaks on a small price gap (a classic bad-debt source). The
// post-action effective LTV is a State₂ value the reducer computes
// (recompute_ltv over refrigerated collaterals/debts + price). The manifest
// declares `position.ltv_after`, projected into `context.custom.ltvAfter`.
// Defined-not-wired → `optional` (fail-open). The 0.90 ceiling stands in for the
// category cap minus the user's leverage buffer (user config).
@id("aave-emode-leverage-warn")
@severity("warn")
@reason("Entering this eMode category would push your effective leverage dangerously high")
forbid(principal, action == Lending::Action::"SetEMode", resource)
when {
  context has custom
  && context.custom has ltvAfter
  && context.custom.ltvAfter.greaterThan(decimal("0.9000"))
};

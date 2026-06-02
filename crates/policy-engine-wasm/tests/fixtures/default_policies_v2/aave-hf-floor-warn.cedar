// scopeball default v2 policy — explorer id: AAVE-01 (health-factor floor).
//
// Rule: warn if, AFTER this borrow, the Aave health factor would fall below the
// user's floor (1.1000). calldata alone cannot answer this — the post-trade HF
// is a State₂ value the reducer must recompute from the wallet's collaterals,
// debts, liquidation thresholds and refrigerated prices. So the manifest
// declares the STATE fact `position.health_factor_after` (Aggregator kind:
// reducer / State₂), served by simulation-server. Defined-not-implemented →
// `optional` (fail-open no-op). This fixture proves the .cedar + manifest are
// consistent and SPECIFIES the method sim-server must serve. Floor is user config.
@id("aave-hf-floor-warn")
@severity("warn")
@reason("After this borrow your Aave health factor would drop below your floor")
forbid(principal, action == Lending::Action::"Borrow", resource)
when {
  context has custom
  && context.custom has healthFactorAfter
  && context.custom.healthFactorAfter.lessThan(decimal("1.1000"))
};

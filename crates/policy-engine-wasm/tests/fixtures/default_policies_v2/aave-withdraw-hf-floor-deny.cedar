// dambi default v2 policy — explorer id: AAVE-08 (withdraw silently turns off
// collateral and drops HF below the floor).
//
// STATE / REDUCER fact — REUSES the existing `position.health_factor_after`
// method (the same fact AAVE-01 drives, here on the withdraw path). Aave auto-
// disables an asset's collateral use when its balance hits 0 or HF conditions
// trip, so a 'harmless' partial withdraw can shrink the collateral base and
// invite liquidation — invisible from calldata. The reducer applies the withdraw
// to State₁ (reduce_collateral) and recomputes HF over the State₂ collateral set
// + prices + liquidation thresholds, projected into `context.custom.healthFactorAfter`.
// Defined-not-wired → `optional` (fail-open). Floor is user config.
@id("aave-withdraw-hf-floor-deny")
@severity("deny")
@reason("After this withdrawal your Aave health factor would drop below your floor")
forbid(principal, action == Lending::Action::"Withdraw", resource)
when {
  context has custom
  && context.custom has healthFactorAfter
  && context.custom.healthFactorAfter.lessThan(decimal("1.1000"))
};

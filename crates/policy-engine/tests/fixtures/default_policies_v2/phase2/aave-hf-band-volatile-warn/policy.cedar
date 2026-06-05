// scopeball default v2 policy — explorer id: AAVE-02 (post-trade HF in the warn
// band AND collateral is volatile).
//
// STATE / REDUCER fact. A "permitted but not safe" HF band (e.g. 1.1–1.5) is far
// riskier when the collateral is a volatile asset than a stablecoin — one price
// dip and you're liquidated. calldata cannot answer this: the post-trade HF is a
// State₂ value the reducer recomputes, and the collateral-volatility class is an
// ExternalInfo tag joined onto the refrigerated collateral set. The manifest
// declares `position.health_factor_with_volatility`, a record method emitting
// BOTH `healthFactorAfter` (decimal) and `collateralIsVolatile` (Bool) from one
// call. Defined-not-wired → `optional` (fail-open). Floor/warn band is user config.
@id("aave-hf-band-volatile-warn")
@severity("warn")
@reason("Your post-borrow health factor is in the warn band and your collateral is volatile")
forbid(principal, action == Lending::Action::"Borrow", resource)
when {
  context has custom
  && context.custom has healthFactorAfter
  && context.custom has collateralIsVolatile
  && context.custom.healthFactorAfter.greaterThanOrEqual(decimal("1.1000"))
  && context.custom.healthFactorAfter.lessThan(decimal("1.5000"))
  && context.custom.collateralIsVolatile
};

// scopeball default v2 policy — explorer id: PERP-10 (total cross-margin exposure
// over your account-wide limit).
//
// Cross positions all share one collateral pool, so a new entry stacks on top of
// every existing cross position — one bad trade can pull in the others' equity.
// The numerator folds ALL held cross positions' notional (each position's
// size_base × its mark, State₁ + Oracle) PLUS the new order's notional; the
// denominator is account collateral. This is a whole-(perp)-account FOLD across
// `positions`, not a single live_inputs read, and the per-position notionals are
// U256-hex Strings — Cedar can neither sum nor divide them. The manifest's
// `perp.cross_total_exposure_ratio` performs the fold and emits the `decimal`
// `crossTotalExposureRatio`. Defined-not-wired → `optional` (fail-open). Warn
// when total cross exposure exceeds 3x collateral (user config).
@id("perp-cross-exposure-cap-warn")
@severity("warn")
@reason("Your total cross-margin exposure would exceed your account-wide limit")
forbid(principal, action == Perp::Action::"OpenPosition", resource)
when {
  context has custom
  && context.custom has crossTotalExposureRatio
  && context.custom.crossTotalExposureRatio.greaterThan(decimal("3.0000"))
};

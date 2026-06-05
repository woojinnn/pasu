// scopeball default v2 policy — explorer id: LP-04 (bonding-curve buy at an
// excessive price premium).
//
// On pump.fun-style bonding-curve launches, sniper bots front-run and inflate
// the price, leaving late buyers filling at the top. The defence prices the
// fair value from the CURRENT curve reserves and compares it to what the user
// is willing to pay (`amount` / `expectedTokenPrice` on the Commit), denying
// when the premium is excessive.
//
// sv = LI per the explorer: no durable state — the curve reserves are an
// external/oracle read, and Cedar cannot derive a premium in basis points from
// reserves + amounts. So this is an EXTERNAL method
// (`launchpad.curve_premium_bp`) projecting `context.custom.curvePremiumBp`
// (Long). The call is `optional`: until the oracle/curve provider ships, its
// absence fail-opens (`context.custom has curvePremiumBp` is false -> no-op).
// The threshold below is a placeholder for the user's `curve_max_premium_bp`.
@id("lp-bonding-curve-premium-deny")
@severity("deny")
@reason("The bonding-curve price premium over fair value is too high")
forbid(principal, action == Launchpad::Action::"Commit", resource)
when {
  context has custom
  && context.custom has curvePremiumBp
  && context.custom.curvePremiumBp > 2000
};

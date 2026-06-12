// dambi default v2 policy — explorer id: E1 / LIQ (stable reserve floor).
// SAME fold fact `portfolio.group_pct`, group_axis=category with asset="stable"
// (the Stable category, a literal group key). Warn if, AFTER the swap, the
// stablecoin reserve share would drop below the user's floor. Note the LESS-than
// comparison — the reserve must stay HIGH. Floor is user config.
@id("portfolio-stable-reserve-floor-warn")
@severity("warn")
@reason("After this swap your stablecoin reserve would drop below your floor")
forbid(principal, action == Amm::Action::"Swap", resource)
when {
  context has custom
  && context.custom has stableReservePctAfter
  && context.custom.stableReservePctAfter.lessThan(decimal("10.0000"))
};

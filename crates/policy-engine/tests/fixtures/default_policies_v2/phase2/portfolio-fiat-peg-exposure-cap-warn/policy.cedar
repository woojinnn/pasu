// scopeball default v2 policy — explorer id: A4 / CONC (fiat-peg exposure).
// SAME fold fact `portfolio.group_pct`, group_axis=fiat_peg: warn if too much of
// the portfolio sits in one fiat peg (e.g. USD-pegged stables) after the swap.
// peg_to.Fiat is first-class in the state model. Cap is user config.
@id("portfolio-fiat-peg-exposure-cap-warn")
@severity("warn")
@reason("After this swap your exposure to this fiat peg would exceed your cap")
forbid(principal, action == Amm::Action::"Swap", resource)
when {
  context has custom
  && context.custom has fiatPegPctAfter
  && context.custom.fiatPegPctAfter.greaterThan(decimal("80.0000"))
};

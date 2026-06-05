// scopeball default v2 policy — explorer id: A2 / CONC (category concentration).
// Sibling of A1: SAME state fact `portfolio.group_pct`, group_axis=category.
// Warn if, after the swap, the acquired token's category (governance/meme/etc.)
// would exceed the user's per-category share-of-portfolio cap. Needs whole-wallet
// USD (State) → manifest declares the fold fact (optional = defined-not-wired).
// Cap is user config.
@id("portfolio-category-concentration-cap-warn")
@severity("warn")
@reason("After this swap your exposure to this token category would exceed your cap")
forbid(principal, action == Amm::Action::"Swap", resource)
when {
  context has custom
  && context.custom has categoryPctAfter
  && context.custom.categoryPctAfter.greaterThan(decimal("40.0000"))
};

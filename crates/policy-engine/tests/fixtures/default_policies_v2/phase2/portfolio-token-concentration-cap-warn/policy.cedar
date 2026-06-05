// scopeball default v2 policy — explorer id: A1 / CONC (single-token
// concentration cap). Personal-trading family.
//
// Rule: after this swap, if the acquired token would exceed the user's
// share-of-portfolio cap, flag it.
//
// WHY a manifest method is required (the point of this fixture):
//   calldata parsing alone CANNOT answer "what % of my whole portfolio is this
//   token after the trade" — that needs every holding valued in USD (State).
//   So the manifest declares a STATE fact `portfolio.group_pct` that
//   simulation-server serves by folding the whole wallet (Aggregator kind:
//   fold, basis: State₂). The fact is a DEFINITION here, not yet implemented,
//   so the call is `optional` (fail-open no-op): this fixture proves the
//   .cedar + manifest are mutually consistent and SPECIFIES the method
//   simulation-server must be able to serve. (Cap "25.0000" is user config.)
@id("portfolio-token-concentration-cap-warn")
@severity("warn")
@reason("After this swap, one token would exceed your portfolio-concentration cap")
forbid(principal, action == Amm::Action::"Swap", resource)
when {
  context has custom
  && context.custom has portfolioTokenPctAfter
  && context.custom.portfolioTokenPctAfter.greaterThan(decimal("25.0000"))
};

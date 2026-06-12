// dambi default v2 policy — explorer id: H2 / VALUE (belief-based exclusion).
// Personal-trading (values screening) family.
//
// Rule: deny acquiring an interest-bearing (riba) token for a user whose profile
// excludes interest income — e.g. an Islamic-finance no-interest principle.
//
// WHY a method (and why this is the COVERED half of H2):
//   "interest-bearing" is STRUCTURALLY identifiable from state: TokenKind carries
//   YieldReceipt / DebtReceipt / StakeReceipt variants plus a RateMode, so the
//   token's interest nature is a direct state read — no external feed needed. The
//   acquired token (`tokenOut`) is a Core::TokenRef record, so Cedar cannot read
//   its kind from the action; the manifest declares `token.interest_bearing`,
//   projecting the Bool into context.custom. Defined-not-wired → `optional`
//   (fail-open), so the field is `has`-guarded.
//
//   The OTHER half of H2 — content screening (gambling / adult) — needs an
//   external protocol→content classifier and is a separate enrichment (the GAP
//   slice); this fixture ships the state-covered interest-bearing slice.
@id("values-interest-bearing-exclude-warn")
@severity("warn")
@reason("This token earns or pays interest — your values profile excludes interest-bearing assets")
forbid(principal, action == Amm::Action::"Swap", resource)
when {
  context has custom
  && context.custom has tokenIsInterestBearing
  && context.custom.tokenIsInterestBearing
};

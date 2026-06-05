// scopeball default v2 policy — explorer id: F4 / ORDER (FOMO pump chasing).
// Personal-trading (TradFi pre-trade) family.
//
// Rule: warn when the token being BOUGHT (`tokenOut`) has already run up sharply
// over the last few days — buying it now at market is textbook top-chasing.
//
// WHY a method is required:
//   recent price momentum is a price-feed signal over a trailing window, not a
//   field of the swap context. The manifest declares
//   `external.token_recent_return_pct` (a 5-day return from an oracle/momentum
//   feed), projecting it into the `decimal` `recentReturnPct`. Defined-not-wired
//   → `optional` (fail-open), so the field is `has`-guarded; decimal uses
//   `.greaterThan(...)`. The full F4 enforcement (a 24h cooldown timer) needs the
//   #7 history surface that does not exist yet, but the verdict itself is fully
//   expressible as a warn — so this fixture ships the signal half. +40% over 5
//   days is user config.
@id("behav-fomo-pump-chase-warn")
@severity("warn")
@reason("This token already pumped hard recently — buying the spike at market is chasing the top")
forbid(principal, action == Amm::Action::"Swap", resource)
when {
  context has custom
  && context.custom has recentReturnPct
  && context.custom.recentReturnPct.greaterThan(decimal("40.0000"))
};

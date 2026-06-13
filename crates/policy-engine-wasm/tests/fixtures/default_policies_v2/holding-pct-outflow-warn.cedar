// dambi default v2 policy — explorer id: GEN-06 (moving too large a share of
// one asset's holding in a single action).
//
// Sending or approving a very large fraction of what you hold of one token is a
// classic drain signature (and an easy fat-finger). The share is
// `amount / token_balance × 100` — the proposed action's `amount` over the
// wallet's CURRENT holding of that exact token (Aggregator type 1 — State read:
// the denominator is `token_holdings.balance_amount`, not the whole portfolio,
// so this is distinct from the portfolio-fold `portfolio.group_pct`). `amount`
// is a raw U256 hex String (NOT Cedar-comparable) and the division needs the
// state balance, so the percentage is computed off-Cedar: the manifest's
// `token.outflow_pct_of_holding` policy_rpc call projects it into
// `context.custom.pctOfHolding`.
//
// `custom` is optional, so the guard `has`-checks the path; the value is a Cedar
// `decimal` extension, so the threshold uses `.greaterThan(...)`. The balance
// read lives in the sim-server fact host, so the call is `optional` and the
// policy fail-opens until it ships. The threshold is user configuration; for this
// slice it is inlined (90 ⇒ warn when over 90% of the holding moves at once).
@id("holding-pct-outflow-warn")
@severity("warn")
@reason("This moves a very large share of your holding of this token in one go")
forbid(principal, action == Token::Action::"Erc20Transfer", resource)
when {
  context has custom
  && context.custom has pctOfHolding
  && context.custom.pctOfHolding.greaterThan(decimal("90.0000"))
};

// scopeball default v2 policy — explorer id: GEN-08 (gas cost disproportionate
// to the value moved).
//
// A swap whose gas fee is a large fraction of the value it actually moves is
// either dust-attack bait or a mis-priced transaction the user should reconsider.
// The ratio is `gas_usd / action_value_usd` — two USD valuations divided
// (Aggregator type 2 — Valuation, combined). Cedar cannot divide two `decimal`
// values, so the division is done off-Cedar: the manifest's
// `valuation.gas_cost_ratio` policy_rpc call projects the ratio into
// `context.custom.gasCostRatioOfValue`.
//
// `custom` is optional, so the guard `has`-checks the path; the value is a Cedar
// `decimal` extension, so the threshold uses `.greaterThan(...)`. The native +
// input price reads live in the sim-server fact host, so the call is `optional`
// and the policy fail-opens until it ships. The threshold is user configuration;
// for this slice it is inlined (0.2 ⇒ warn when gas exceeds 20% of the swap's
// value).
//
// Scoped to `Amm::Action::"Swap"` because the ratio's denominator (the swap's
// input value) and `gasEstimate` are both swap-context fields today.
@id("gas-cost-ratio-warn")
@severity("warn")
@reason("The gas fee is a large fraction of the value this swap moves")
forbid(principal, action == Amm::Action::"Swap", resource)
when {
  context has custom
  && context.custom has gasCostRatioOfValue
  && context.custom.gasCostRatioOfValue.greaterThan(decimal("0.2000"))
};

// dambi default v2 policy — explorer id: GEN-07 (absolute gas cost over a
// USD cap).
//
// An abnormally expensive transaction — fat-fingered gas, a malicious relayer
// padding the fee, or a griefing contract — should be stopped before signing.
// Gas cost in USD is `gasEstimate × native_token price_value` (Aggregator type 2
// — Valuation): the swap's `gasEstimate` is a raw U256 hex String on the base
// context (NOT Cedar-comparable), so the USD product is computed off-Cedar and
// the manifest's `valuation.gas_cost_usd` policy_rpc call projects it into
// `context.custom.gasCostUsd`.
//
// `custom` is optional, so the guard `has`-checks the path; the value is a Cedar
// `decimal` extension, so the threshold uses `.greaterThan(...)`. The native
// price read lives in the sim-server fact host, so the call is `optional` and the
// policy fail-opens until it ships. The cap is user configuration; for this slice
// it is inlined ($150) — personalized supply is a later increment.
//
// Scoped to `Amm::Action::"Swap"` because `gasEstimate` is a base field only of
// the swap context today (the only action carrying the route's gas projection).
@id("gas-cost-usd-cap-deny")
@severity("deny")
@reason("The gas fee for this transaction is abnormally high in USD terms")
forbid(principal, action == Amm::Action::"Swap", resource)
when {
  context has custom
  && context.custom has gasCostUsd
  && context.custom.gasCostUsd.greaterThan(decimal("150.0000"))
};

// dambi default v2 policy — explorer id: GEN-05 (whole-tx / multicall net
// outflow over a USD cap).
//
// A single transfer is caught by GEN-04, but a multicall can drain the wallet
// across several children whose individual legs each sit under the per-asset
// cap. This policy keys on the WHOLE transaction: the reducer applies every
// child `ActionBody` in order (State₁ → … → State₂) and the net negative
// balance deltas are valued at `price_value` and summed (Aggregator type 6 —
// reducer State₂ fold). The manifest's `valuation.tx_total_usd` policy_rpc call
// projects that total into `context.custom.txNetOutflowUsd`.
//
// `custom` is optional, so the guard `has`-checks the path; the value is a Cedar
// `decimal` extension, so the threshold uses `.greaterThan(...)`. The reducer /
// price read live in the sim-server fact host, so until it ships the call is
// `optional` and the policy fail-opens (no `txNetOutflowUsd` ⇒ no-op). The cap is
// user configuration; for this slice it is inlined ($25,000) — personalized
// supply is a later increment (design proposal F#2).
//
// Trigger is `action.domain == multicall`: the cap is meaningful only for the
// bundle as a whole (single-action txs are GEN-04's job and stay under it).
@id("multicall-outflow-cap-deny")
@severity("deny")
@reason("This bundle moves more total value out of your wallet than your per-transaction cap")
forbid(principal, action == Core::Action::"Multicall", resource)
when {
  context has custom
  && context.custom has txNetOutflowUsd
  && context.custom.txNetOutflowUsd.greaterThan(decimal("25000.0000"))
};

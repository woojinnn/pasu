// dambi default v2 policy — explorer id: GEN-04 (per-asset USD outflow cap).
//
// Deny an ERC20 transfer whose USD value exceeds the user's per-asset spend cap
// (a product-team-requested control). The outflow is valued by the oracle
// (Aggregator type 2 — Valuation = Action amount × Oracle price): the manifest's
// `oracle.usd_value` policy_rpc call (served by the external policy-rpc host)
// projects the transfer's fiat value into `context.custom.outflowAssetUsd`.
//
// `custom` is optional, so the guard `has`-checks the path; the value is a Cedar
// `decimal` extension, so the threshold uses `.greaterThan(...)`. The cap is user
// configuration; for this slice it is inlined ($10,000) — personalized supply is
// a later increment (design proposal F#2).
@id("transfer-outflow-usd-cap")
@severity("deny")
forbid(principal, action == Token::Action::"Erc20Transfer", resource)
when {
  context has custom
  && context.custom has outflowAssetUsd
  && context.custom.outflowAssetUsd.greaterThan(decimal("10000.0000"))
};

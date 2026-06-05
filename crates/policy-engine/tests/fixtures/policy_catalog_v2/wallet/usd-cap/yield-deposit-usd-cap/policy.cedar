// blocks: minting SY (yield-vault deposit) above the $50k USD cap in one action.
// why:    deposit-concentration risk — a large yield deposit locks funds into one
//         protocol's SY/PT/YT contract surface and maturity risk; a USD ceiling
//         forces a size confirmation. source: Fireblocks transaction-authorization
//         USD limits.
// bucket: wallet/usd-cap
// tags: bucket:wallet cat:usd-cap enrichment method:oracle.usd_value action-family:usd-cap
// methods: oracle.usd_value -> ../../../_methods/oracle.usd_value.md
//
// VOCAB NOTE: Yield::MintSy is the SY-deposit entrypoint; the deposited asset is
// `externalToken` and the deposited amount is `netTokenIn` (VOCAB §4 yield).
// `oracle.usd_value` enriches that token+amount into `context.custom.yieldDepositUsd`.
// ENRICHMENT — dormant until the /v1/rpc dispatcher serves `oracle.usd_value`.
@id("yield-deposit-usd-cap")
@severity("warn")
@reason("Large yield deposit (over $50k) — confirm the deposit size before signing")
forbid(principal, action == Yield::Action::"MintSy", resource)
when {
  context has custom
  && context.custom has yieldDepositUsd
  && context.custom.yieldDepositUsd.greaterThan(decimal("50000.0000"))
};

// blocks: claiming an airdrop whose token value exceeds the $50k USD cap.
// why:    high-value-claim confirmation — an unexpectedly large claim is a strong
//         signal to inspect the distributor / proof / recipient before signing
//         (scam "claim" lures often spoof large amounts); a USD ceiling surfaces it.
//         source: Fireblocks transaction-authorization USD limits.
// bucket: wallet/usd-cap
// tags: bucket:wallet cat:usd-cap enrichment method:oracle.usd_value action-family:usd-cap
// methods: oracle.usd_value -> ../../../_methods/oracle.usd_value.md
//
// VOCAB NOTE: Airdrop::Claim carries `claimToken` + `actualAmount` (VOCAB §4 airdrop).
// `oracle.usd_value` enriches that token+amount into `context.custom.claimUsd`.
// ENRICHMENT — dormant until the /v1/rpc dispatcher serves `oracle.usd_value`.
@id("airdrop-claim-usd-cap")
@severity("warn")
@reason("Large airdrop claim (over $50k) — verify the distributor and recipient before signing")
forbid(principal, action == Airdrop::Action::"Claim", resource)
when {
  context has custom
  && context.custom has claimUsd
  && context.custom.claimUsd.greaterThan(decimal("50000.0000"))
};

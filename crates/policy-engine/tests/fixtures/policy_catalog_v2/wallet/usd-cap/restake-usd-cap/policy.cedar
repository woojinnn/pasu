// blocks: depositing an LST into a restaking strategy above the $50k USD cap.
// why:    deposit-concentration risk — a large restake deposit adds AVS-slashing
//         exposure on top of the underlying LST's risk; a USD ceiling forces a size
//         confirmation. source: Fireblocks transaction-authorization USD limits.
// bucket: wallet/usd-cap
// tags: bucket:wallet cat:usd-cap enrichment method:oracle.usd_value action-family:usd-cap
// methods: oracle.usd_value -> ../../../_methods/oracle.usd_value.md
//
// VOCAB NOTE: Restaking::Deposit carries `token` (the LST) + `amount` (VOCAB §4).
// `oracle.usd_value` enriches that token+amount into `context.custom.restakeUsd`.
// ENRICHMENT — dormant until the /v1/rpc dispatcher serves `oracle.usd_value`.
@id("restake-usd-cap")
@severity("warn")
@reason("Large restake deposit (over $50k) — confirm the deposit size before signing")
forbid(principal, action == Restaking::Action::"Deposit", resource)
when {
  context has custom
  && context.custom has restakeUsd
  && context.custom.restakeUsd.greaterThan(decimal("50000.0000"))
};

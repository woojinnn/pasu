// blocks: withdrawing supplied funds from a lending market above the $50k USD cap.
// why:    large-outflow confirmation — an oversized withdraw (or uint-max full pull)
//         drains a position and can breach a health buffer; a USD ceiling forces a
//         size confirmation. source: Fireblocks transaction-authorization USD limits.
// bucket: wallet/usd-cap
// tags: bucket:wallet cat:usd-cap enrichment method:oracle.usd_value action-family:usd-cap
// methods: oracle.usd_value -> ../../../_methods/oracle.usd_value.md
//
// `oracle.usd_value` enriches the withdrawn asset+amount into `context.custom.withdrawUsd`.
// ENRICHMENT — dormant until the /v1/rpc dispatcher serves `oracle.usd_value`.
@id("withdraw-usd-cap")
@severity("warn")
@reason("Large withdraw (over $50k) — confirm the amount and your remaining position")
forbid(principal, action == Lending::Action::"Withdraw", resource)
when {
  context has custom
  && context.custom has withdrawUsd
  && context.custom.withdrawUsd.greaterThan(decimal("50000.0000"))
};

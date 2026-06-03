// blocks: repaying a debt position above the $50k USD cap in one action.
// why:    large-outflow confirmation — an oversized repay (or a uint-max full repay
//         mis-priced large) moves significant funds; a USD ceiling forces a size
//         confirmation before signing. source: Fireblocks transaction-authorization
//         USD limits.
// bucket: wallet/usd-cap
// tags: bucket:wallet cat:usd-cap enrichment method:oracle.usd_value action-family:usd-cap
// methods: oracle.usd_value -> ../../../_methods/oracle.usd_value.md
//
// `oracle.usd_value` enriches the repaid asset+amount into `context.custom.repayUsd`.
// ENRICHMENT — dormant until the /v1/rpc dispatcher serves `oracle.usd_value`.
@id("lending-repay-usd-cap")
@severity("warn")
@reason("Large repay (over $50k) — confirm the amount before signing")
forbid(principal, action == Lending::Action::"Repay", resource)
when {
  context has custom
  && context.custom has repayUsd
  && context.custom.repayUsd.greaterThan(decimal("50000.0000"))
};

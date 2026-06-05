// blocks: supplying collateral into a lending market above the $50k USD cap.
// why:    deposit-concentration risk — a large single supply concentrates funds in
//         one protocol's smart-contract / depeg / freeze surface; a USD ceiling
//         forces a size confirmation. source: Fireblocks transaction-authorization
//         USD limits.
// bucket: wallet/usd-cap
// tags: bucket:wallet cat:usd-cap enrichment method:oracle.usd_value action-family:usd-cap
// methods: oracle.usd_value -> ../../../_methods/oracle.usd_value.md
//
// `oracle.usd_value` enriches the supplied asset+amount into `context.custom.supplyUsd`.
// ENRICHMENT — dormant until the /v1/rpc dispatcher serves `oracle.usd_value`.
@id("lending-supply-usd-cap")
@severity("warn")
@reason("Large lending supply (over $50k) — confirm the deposit size before signing")
forbid(principal, action == Lending::Action::"Supply", resource)
when {
  context has custom
  && context.custom has supplyUsd
  && context.custom.supplyUsd.greaterThan(decimal("50000.0000"))
};

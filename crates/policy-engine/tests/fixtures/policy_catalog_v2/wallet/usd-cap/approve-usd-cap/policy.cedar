// blocks: granting an ERC-20 spend approval whose USD notional exceeds the $25k cap.
// why:    approval-sizing risk — a large allowance is a standing pull-right a drainer
//         can exercise later; a USD ceiling forces a size confirmation before signing.
//         source: Fireblocks transaction-authorization USD limits.
// bucket: wallet/usd-cap
// tags: bucket:wallet cat:usd-cap enrichment method:oracle.usd_value action-family:usd-cap
// methods: oracle.usd_value -> ../../../_methods/oracle.usd_value.md
//
// `oracle.usd_value` enriches the approved token+amount into `context.custom.approveUsd`.
// ENRICHMENT — dormant until the /v1/rpc dispatcher serves `oracle.usd_value`.
@id("approve-usd-cap")
@severity("warn")
@reason("Large approval (over $25k notional) — confirm the spender and size before signing")
forbid(principal, action == Token::Action::"Erc20Approve", resource)
when {
  context has custom
  && context.custom has approveUsd
  && context.custom.approveUsd.greaterThan(decimal("25000.0000"))
};

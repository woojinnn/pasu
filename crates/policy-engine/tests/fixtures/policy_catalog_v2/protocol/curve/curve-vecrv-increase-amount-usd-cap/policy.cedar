// blocks: topping up a veCRV lock with more than $25k of CRV in one increase_amount
// why:    a large veCRV top-up commits significant value to an irreversible time-lock; a USD ceiling forces an explicit size confirmation before the funds become illiquid. source: Curve DAO VotingEscrow docs (increase_amount); Fireblocks transaction-authorization USD limits.
// bucket: protocol/curve
// tags: bucket:protocol cat:curve enrichment method:oracle.usd_value venue:curve action-family:usd-cap
// methods: oracle.usd_value -> ../../../_methods/oracle.usd_value.md
//
// VOCAB NOTE: Staking::IncreaseLockAmount carries `token` (CRV) + `amount` (VOCAB §4).
// `oracle.usd_value` enriches that token+amount into `context.custom.usdValue`.
// ENRICHMENT — dormant until the /v1/rpc dispatcher serves `oracle.usd_value`.
@id("curve-vecrv-increase-amount-usd-cap")
@severity("warn")
@reason("Large veCRV top-up (over $25k) into an irreversible lock — confirm the size before signing")
forbid(principal, action == Staking::Action::"IncreaseLockAmount", resource)
when {
  context has custom
  && context.custom has usdValue
  && context.custom.usdValue.greaterThan(decimal("25000.0000"))
};

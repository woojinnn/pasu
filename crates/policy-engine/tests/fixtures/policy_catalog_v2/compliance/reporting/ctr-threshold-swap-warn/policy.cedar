// blocks: an AMM swap whose input USD notional crosses the BSA Currency-Transaction-Report $10,000 threshold — informational warn.
// why:    reporting-threshold (informational) — the US Bank Secrecy Act requires financial institutions to file a Currency Transaction Report (CTR) for cash transactions over $10,000. A self-custody signer is NOT a financial institution and owes no CTR, so this is a WARN that surfaces the swap would be a CTR-size transaction at a regulated entity — not a block. Notional is taken from the swap INPUT (tokenIn × amountIn). (FinCEN BSA / 31 CFR 1010.311.)
// bucket: compliance/reporting
// tags: bucket:compliance cat:reporting enrichment method:oracle.usd_value could-be-compliance:true action-family:swap
// methods: oracle.usd_value -> ../../../_methods/oracle.usd_value.md
//
// `oracle.usd_value` enriches the swap input notional (asset=tokenIn, amount=direction.amountIn)
// into `context.custom.usdValue` (Decimal). Compares with a 4-dp decimal literal
// (VOCAB: Cedar Decimal max 4 dp). ENRICHMENT — dormant until the dispatcher serves
// `oracle.usd_value`.
@id("ctr-threshold-swap-warn")
@severity("warn")
@reason("Swap exceeds the BSA CTR $10,000 threshold (informational — regulated-entity reporting size)")
forbid(principal, action == Amm::Action::"Swap", resource)
when {
  context has custom
  && context.custom has usdValue
  && context.custom.usdValue.greaterThan(decimal("10000.0000"))
};

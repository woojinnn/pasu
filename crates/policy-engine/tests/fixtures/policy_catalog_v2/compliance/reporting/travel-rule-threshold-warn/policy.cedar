// blocks: an ERC-20 transfer whose USD notional crosses the FATF Travel-Rule $1,000 threshold — informational warn.
// why:    reporting-threshold (informational) — FATF Recommendation 16 sets a USD/EUR 1,000 threshold above which VASPs must transmit originator/beneficiary info. A self-custody signer is NOT a VASP and carries no such duty, so this is a WARN that merely surfaces that the transfer would be a reportable-size transaction at a VASP — not a block. (FATF Recommendation 16.)
// bucket: compliance/reporting
// tags: bucket:compliance cat:reporting enrichment method:oracle.usd_value could-be-compliance:true action-family:transfer
// methods: oracle.usd_value -> ../../../_methods/oracle.usd_value.md
//
// `oracle.usd_value` enriches the transfer notional into `context.custom.usdValue`
// (Decimal). Compares with 4-dp decimal literal (VOCAB: Cedar Decimal max 4 dp).
// ENRICHMENT — dormant until the dispatcher serves `oracle.usd_value`.
@id("travel-rule-threshold-warn")
@severity("warn")
@reason("Transfer exceeds the FATF Travel-Rule $1,000 threshold (informational — VASP reporting size)")
forbid(principal, action == Token::Action::"Erc20Transfer", resource)
when {
  context has custom
  && context.custom has usdValue
  && context.custom.usdValue.greaterThan(decimal("1000.0000"))
};

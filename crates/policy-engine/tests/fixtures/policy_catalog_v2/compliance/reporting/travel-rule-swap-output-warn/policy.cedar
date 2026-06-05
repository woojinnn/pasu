// blocks: a swap routing >$1,000 of output to a recipient that is NOT the signer — informational Travel-Rule warn.
// why:    reporting-threshold (informational) — a swap that sends proceeds to a third party (recipient != submitter) above the FATF Recommendation 16 $1,000 threshold is a Travel-Rule-size third-party transfer at a VASP. A self-custody signer owes no Travel-Rule duty, so this is a WARN surfacing the size + third-party routing — not a block. Same-recipient (self) swaps are not flagged. (FATF Recommendation 16.)
// bucket: compliance/reporting
// tags: bucket:compliance cat:reporting enrichment method:oracle.usd_value could-be-compliance:true action-family:swap
// methods: oracle.usd_value -> ../../../_methods/oracle.usd_value.md
//
// `oracle.usd_value` enriches the swap input notional into `context.custom.usdValue`
// (Decimal). `recipient` and `meta.submitter` are non-optional Strings on Swap
// (VOCAB §4 amm), so the third-party check needs no `has` guard. 4-dp decimal literal.
// ENRICHMENT — dormant until the dispatcher serves `oracle.usd_value`.
@id("travel-rule-swap-output-warn")
@severity("warn")
@reason("Swap sends over $1,000 to a third-party recipient (informational — Travel-Rule size)")
forbid(principal, action == Amm::Action::"Swap", resource)
when {
  context.recipient != context.meta.submitter
  && context has custom
  && context.custom has usdValue
  && context.custom.usdValue.greaterThan(decimal("1000.0000"))
};

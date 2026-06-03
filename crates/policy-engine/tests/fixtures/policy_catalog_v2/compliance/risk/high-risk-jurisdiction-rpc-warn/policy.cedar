// blocks: an un-decoded call whose target contract is attributed to a high-risk / FATF-listed jurisdiction — soft warn.
// why:    risk-category screening — a target tied to a FATF "high-risk / increased-monitoring" jurisdiction warrants enhanced scrutiny but is not per-se prohibited, so this is a WARN. The Unknown catch-all is where un-decoded calldata to such a target lands. (FATF high-risk & monitored jurisdictions list; analytics category = high_risk_jurisdiction.)
// bucket: compliance/risk
// tags: bucket:compliance cat:risk enrichment method:address.category could-be-compliance:true action-family:unknown
// methods: address.category -> ../../../_methods/address.category.md
// dormant: until address.category is registered in method-catalog.json
//
// `address.category` projects a chain-analytics jurisdiction-risk label for the
// call target into `context.custom.riskCategory`. Heightened-scrutiny, not
// prohibition → warn tier. ENRICHMENT — dormant until wired.
@id("high-risk-jurisdiction-rpc-warn")
@severity("warn")
@reason("Target is attributed to a high-risk jurisdiction (FATF) — review before proceeding")
forbid(principal, action == Core::Action::"Unknown", resource)
when {
  context has custom
  && context.custom has riskCategory
  && context.custom.riskCategory == "high_risk_jurisdiction"
};

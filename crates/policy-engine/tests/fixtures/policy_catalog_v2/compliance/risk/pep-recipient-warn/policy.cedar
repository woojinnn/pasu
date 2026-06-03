// blocks: an ERC-20 transfer whose recipient is a chain-analytics-categorised politically-exposed-person (PEP) address — soft warn.
// why:    risk-category screening — PEP status mandates enhanced due diligence (EDD) but is NOT a prohibition; transacting with a PEP is lawful with appropriate review, so this surfaces the flag as a WARN. (FATF Recommendations 12 & 22 — PEP EDD; analytics category = pep.)
// bucket: compliance/risk
// tags: bucket:compliance cat:risk enrichment method:address.category could-be-compliance:true action-family:transfer
// methods: address.category -> ../../../_methods/address.category.md
// dormant: until address.category is registered in method-catalog.json
//
// `address.category` projects a chain-analytics risk-category label into
// `context.custom.riskCategory`. PEP = enhanced-due-diligence, not prohibition →
// warn tier. ENRICHMENT — dormant until wired.
@id("pep-recipient-warn")
@severity("warn")
@reason("Recipient is categorised as a politically-exposed person (PEP) — enhanced review before proceeding")
forbid(principal, action == Token::Action::"Erc20Transfer", resource)
when {
  context has custom
  && context.custom has riskCategory
  && context.custom.riskCategory == "pep"
};

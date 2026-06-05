// blocks: an ERC-20 transfer whose recipient is a chain-analytics-categorised gambling-service address — soft warn.
// why:    risk-category screening — gambling services carry jurisdiction-specific legality and elevated laundering risk; many users transact legitimately so this is a WARN, surfacing the category rather than blocking. (analytics category = gambling.)
// bucket: compliance/risk
// tags: bucket:compliance cat:risk enrichment method:address.category could-be-compliance:true action-family:transfer
// methods: address.category -> ../../../_methods/address.category.md
// dormant: until address.category is registered in method-catalog.json
//
// `address.category` projects a chain-analytics risk-category label into
// `context.custom.riskCategory`. Jurisdiction-dependent → warn tier.
// ENRICHMENT — dormant until wired.
@id("gambling-service-recipient-warn")
@severity("warn")
@reason("Recipient is categorised as a gambling-service address — review before proceeding")
forbid(principal, action == Token::Action::"Erc20Transfer", resource)
when {
  context has custom
  && context.custom has riskCategory
  && context.custom.riskCategory == "gambling"
};

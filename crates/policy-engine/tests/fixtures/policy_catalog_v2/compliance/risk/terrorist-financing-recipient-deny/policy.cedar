// blocks: an ERC-20 transfer whose recipient is a chain-analytics-categorised terrorist-financing address.
// why:    risk-category screening — sending value to an address tagged for terrorist financing is a strict-liability compliance violation (CFT) and an irreversible loss; analytics providers tag clusters tied to designated FTOs. (FATF Recommendation 5 / CFT; analytics category = terrorist_financing.)
// bucket: compliance/risk
// tags: bucket:compliance cat:risk enrichment method:address.category could-be-compliance:true action-family:transfer
// methods: address.category -> ../../../_methods/address.category.md
// dormant: until address.category is registered in method-catalog.json
//
// `address.category` projects a chain-analytics risk-category label into
// `context.custom.riskCategory`. ENRICHMENT — dormant until wired.
@id("terrorist-financing-recipient-deny")
@severity("deny")
@reason("Recipient is categorised as a terrorist-financing address — transfer blocked")
forbid(principal, action == Token::Action::"Erc20Transfer", resource)
when {
  context has custom
  && context.custom has riskCategory
  && context.custom.riskCategory == "terrorist_financing"
};

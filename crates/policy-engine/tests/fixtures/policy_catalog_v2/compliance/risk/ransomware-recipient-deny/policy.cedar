// blocks: an ERC-20 transfer whose recipient is a chain-analytics-categorised ransomware-payment address.
// why:    risk-category screening — paying a ransomware wallet funds extortion and is frequently sanctioned (OFAC has SDN-listed specific ransomware clusters); analytics providers tag known ransomware deposit addresses. (OFAC ransomware advisory 2021; analytics category = ransomware.)
// bucket: compliance/risk
// tags: bucket:compliance cat:risk enrichment method:address.category could-be-compliance:true action-family:transfer
// methods: address.category -> ../../../_methods/address.category.md
// dormant: until address.category is registered in method-catalog.json
//
// `address.category` projects a chain-analytics risk-category label into
// `context.custom.riskCategory`. ENRICHMENT — dormant until wired.
@id("ransomware-recipient-deny")
@severity("deny")
@reason("Recipient is categorised as a ransomware-payment address — transfer blocked")
forbid(principal, action == Token::Action::"Erc20Transfer", resource)
when {
  context has custom
  && context.custom has riskCategory
  && context.custom.riskCategory == "ransomware"
};

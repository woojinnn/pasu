// blocks: an ERC-20 transfer whose recipient is a chain-analytics-categorised fraud-shop / carding-marketplace address.
// why:    risk-category screening — fraud shops (stolen-card / credential marketplaces) are illicit-finance venues; paying one is both an irreversible loss and a likely-illicit-finance flag. (FATF illicit-finance typologies; analytics category = fraud_shop.)
// bucket: compliance/risk
// tags: bucket:compliance cat:risk enrichment method:address.category could-be-compliance:true action-family:transfer
// methods: address.category -> ../../../_methods/address.category.md
// dormant: until address.category is registered in method-catalog.json
//
// `address.category` projects a chain-analytics risk-category label into
// `context.custom.riskCategory`. ENRICHMENT — dormant until wired.
@id("fraud-shop-recipient-deny")
@severity("deny")
@reason("Recipient is categorised as a fraud-shop address — transfer blocked")
forbid(principal, action == Token::Action::"Erc20Transfer", resource)
when {
  context has custom
  && context.custom has riskCategory
  && context.custom.riskCategory == "fraud_shop"
};

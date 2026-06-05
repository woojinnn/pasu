// blocks: an ERC-20 transfer whose recipient is a chain-analytics-categorised darknet-market address.
// why:    risk-category screening — paying a darknet-market deposit address is both an irreversible loss and a likely-illicit-finance flag; chain-analytics providers (Chainalysis/TRM/Elliptic categories) tag such clusters. (FATF illicit-finance typologies; analytics category = darknet_market.)
// bucket: compliance/risk
// tags: bucket:compliance cat:risk enrichment method:address.category could-be-compliance:true action-family:transfer
// methods: address.category -> ../../../_methods/address.category.md
// dormant: until address.category is registered in method-catalog.json
//
// `address.category` projects a chain-analytics risk-category label into
// `context.custom.riskCategory`. ENRICHMENT — dormant until the dispatcher
// serves the method; the `has riskCategory` guard is inert until then.
@id("darknet-market-recipient-deny")
@severity("deny")
@reason("Recipient is categorised as a darknet-market address — transfer blocked")
forbid(principal, action == Token::Action::"Erc20Transfer", resource)
when {
  context has custom
  && context.custom has riskCategory
  && context.custom.riskCategory == "darknet_market"
};

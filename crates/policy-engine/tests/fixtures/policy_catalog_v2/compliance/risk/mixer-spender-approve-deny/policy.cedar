// blocks: an ERC-20 approve granting allowance to a chain-analytics-categorised mixer/tumbler spender.
// why:    risk-category screening — granting spend authority to a mixer (Tornado-style tumbler) contract hands a pull-allowance to an obfuscation cluster and, for OFAC-designated mixers, a sanctions exposure; chain-analytics providers (Chainalysis/TRM/Elliptic) tag such clusters with a `mixer` category. Deny — an open allowance to a mixer is a standing path to irreversible value flow. (FATF illicit-finance typologies; analytics category = mixer.)
// bucket: compliance/risk
// tags: bucket:compliance cat:risk enrichment method:address.category could-be-compliance:true action-family:approval
// methods: address.category -> ../../../_methods/address.category.md
// dormant: until address.category is registered in method-catalog.json
//
// `address.category` projects a chain-analytics risk-category label for the approve
// spender into `context.custom.riskCategory`. ENRICHMENT — dormant until the
// dispatcher serves the method; the `has riskCategory` guard is inert until then.
@id("mixer-spender-approve-deny")
@severity("deny")
@reason("Approval spender is categorised as a mixer/tumbler address — approval blocked")
forbid(principal, action == Token::Action::"Erc20Approve", resource)
when {
  context has custom
  && context.custom has riskCategory
  && context.custom.riskCategory == "mixer"
};

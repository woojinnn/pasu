// blocks: an AMM swap whose output recipient is a chain-analytics-categorised mixer/tumbler address.
// why:    risk-category screening — routing swap proceeds into a mixer (Tornado-style tumbler) cluster is a structuring/obfuscation flag and, for OFAC-designated mixers, a sanctions exposure; chain-analytics providers (Chainalysis/TRM/Elliptic) tag such clusters with a `mixer` category. Screen the swap output recipient just like a direct transfer. (FATF illicit-finance typologies; analytics category = mixer.)
// bucket: compliance/risk
// tags: bucket:compliance cat:risk enrichment method:address.category could-be-compliance:true action-family:swap
// methods: address.category -> ../../../_methods/address.category.md
// dormant: until address.category is registered in method-catalog.json
//
// `address.category` projects a chain-analytics risk-category label for the swap
// output recipient into `context.custom.riskCategory`. ENRICHMENT — dormant until
// the dispatcher serves the method; the `has riskCategory` guard is inert until then.
@id("mixer-swap-recipient-deny")
@severity("deny")
@reason("Swap output recipient is categorised as a mixer/tumbler address — swap blocked")
forbid(principal, action == Amm::Action::"Swap", resource)
when {
  context has custom
  && context.custom has riskCategory
  && context.custom.riskCategory == "mixer"
};

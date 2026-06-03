// blocks: an un-decoded call whose target contract is a chain-analytics-categorised mixer / coin-tumbler (e.g. Tornado-class pools).
// why:    risk-category screening — depositing into a mixer obfuscates fund provenance and is a high illicit-finance signal (several mixers are also OFAC-sanctioned); the Unknown catch-all is where un-decoded calldata to such a target lands. (OFAC Tornado Cash designation 2022; analytics category = mixer.)
// bucket: compliance/risk
// tags: bucket:compliance cat:risk enrichment method:address.category could-be-compliance:true action-family:unknown
// methods: address.category -> ../../../_methods/address.category.md
// dormant: until address.category is registered in method-catalog.json
//
// `address.category` projects a chain-analytics risk-category label for the call
// target into `context.custom.riskCategory`. ENRICHMENT — dormant until wired.
@id("mixer-deposit-deny")
@severity("deny")
@reason("Target contract is categorised as a mixer / coin-tumbler — interaction blocked")
forbid(principal, action == Core::Action::"Unknown", resource)
when {
  context has custom
  && context.custom has riskCategory
  && context.custom.riskCategory == "mixer"
};

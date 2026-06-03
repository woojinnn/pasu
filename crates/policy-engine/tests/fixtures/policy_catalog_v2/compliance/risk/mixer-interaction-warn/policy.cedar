// blocks: an un-decoded call whose target is a chain-analytics-categorised mixer — soft warn (interaction, not necessarily a deposit).
// why:    risk-category screening — interacting with a mixer (claims, withdrawals, relayer ops, governance) carries taint/exposure risk but is not always an illicit deposit and not all mixers are sanctioned, so this surfaces the category as a WARN. Pairs with mixer-deposit-deny (the hard block). (analytics category = mixer.)
// bucket: compliance/risk
// tags: bucket:compliance cat:risk enrichment method:address.category could-be-compliance:true action-family:unknown
// methods: address.category -> ../../../_methods/address.category.md
// dormant: until address.category is registered in method-catalog.json
//
// `address.category` projects a chain-analytics risk-category label for the call
// target into `context.custom.riskCategory`. Interaction-level taint → warn tier.
// ENRICHMENT — dormant until wired.
@id("mixer-interaction-warn")
@severity("warn")
@reason("Target contract is categorised as a mixer — review before proceeding")
forbid(principal, action == Core::Action::"Unknown", resource)
when {
  context has custom
  && context.custom has riskCategory
  && context.custom.riskCategory == "mixer"
};

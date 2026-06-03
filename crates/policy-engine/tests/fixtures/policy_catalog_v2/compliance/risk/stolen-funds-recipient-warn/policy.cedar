// blocks: an ERC-20 transfer whose recipient is a chain-analytics-categorised stolen-funds address — soft warn.
// why:    risk-category screening — a "stolen funds" tag means the cluster received proceeds of a known hack/theft; receiving exposure is a taint/laundering risk but the category is provider-attributed and not strict-liability, so WARN not deny. (analytics category = stolen_funds; e.g. post-hack drainer consolidation wallets.)
// bucket: compliance/risk
// tags: bucket:compliance cat:risk enrichment method:address.category could-be-compliance:true action-family:transfer
// methods: address.category -> ../../../_methods/address.category.md
// dormant: until address.category is registered in method-catalog.json
//
// `address.category` projects a chain-analytics risk-category label into
// `context.custom.riskCategory`. Provider-attributed taint → warn tier.
// ENRICHMENT — dormant until wired.
@id("stolen-funds-recipient-warn")
@severity("warn")
@reason("Recipient is categorised as a stolen-funds address — review before proceeding")
forbid(principal, action == Token::Action::"Erc20Transfer", resource)
when {
  context has custom
  && context.custom has riskCategory
  && context.custom.riskCategory == "stolen_funds"
};

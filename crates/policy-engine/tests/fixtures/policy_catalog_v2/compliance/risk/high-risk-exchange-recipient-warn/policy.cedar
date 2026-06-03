// blocks: an ERC-20 transfer whose recipient is a chain-analytics-categorised high-risk / no-KYC exchange deposit address — soft warn.
// why:    risk-category screening — high-risk exchanges (sanctioned/no-KYC/high-illicit-exposure VASPs) are a common off-ramp for illicit funds; depositing there is a heightened-risk signal but legitimate users may transact, so WARN not deny. (FATF VASP guidance; analytics category = high_risk_exchange.)
// bucket: compliance/risk
// tags: bucket:compliance cat:risk enrichment method:address.category could-be-compliance:true action-family:transfer
// methods: address.category -> ../../../_methods/address.category.md
// dormant: until address.category is registered in method-catalog.json
//
// `address.category` projects a chain-analytics risk-category label into
// `context.custom.riskCategory`. Heightened-risk venue → warn tier.
// ENRICHMENT — dormant until wired.
@id("high-risk-exchange-recipient-warn")
@severity("warn")
@reason("Recipient is categorised as a high-risk exchange deposit address — review before proceeding")
forbid(principal, action == Token::Action::"Erc20Transfer", resource)
when {
  context has custom
  && context.custom has riskCategory
  && context.custom.riskCategory == "high_risk_exchange"
};

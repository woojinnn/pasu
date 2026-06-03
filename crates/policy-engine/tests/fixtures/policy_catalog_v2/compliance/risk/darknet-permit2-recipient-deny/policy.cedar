// blocks: signing a Permit2 SignatureTransfer that authorizes a chain-analytics-categorised darknet-market spender.
// why:    risk-category screening — a one-time Permit2 transfer signature whose authorized spender is tagged as a darknet-market cluster hands value to a likely-illicit-finance party off-chain; the recipient is not in the signed struct, so the on-message `spender` is the bind-site. Chain-analytics providers (Chainalysis/TRM/Elliptic) tag such clusters with a `darknet_market` category. (FATF illicit-finance typologies; analytics category = darknet_market.)
// bucket: compliance/risk
// tags: bucket:compliance cat:risk enrichment method:address.category could-be-compliance:true action-family:approval
// methods: address.category -> ../../../_methods/address.category.md
// dormant: until address.category is registered in method-catalog.json
//
// `address.category` projects a chain-analytics risk-category label for the Permit2
// authorized spender into `context.custom.riskCategory`. Off-chain signature
// (`meta.nature.kind == "offchain_sig"`). ENRICHMENT — dormant until the dispatcher
// serves the method; the `has riskCategory` guard is inert until then.
@id("darknet-permit2-recipient-deny")
@severity("deny")
@reason("Permit2 transfer spender is categorised as a darknet-market address — signature blocked")
forbid(principal, action == Token::Action::"Permit2SignTransfer", resource)
when {
  context has custom
  && context.custom has riskCategory
  && context.custom.riskCategory == "darknet_market"
};

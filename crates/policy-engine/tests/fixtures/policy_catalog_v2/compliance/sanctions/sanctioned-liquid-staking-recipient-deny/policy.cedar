// blocks: a liquid-staking share transfer (e.g. Lido stETH transferShares) whose recipient is on an OFAC SDN / EU / UN sanctions list.
// why:    sanctions screening — moving liquid-staking shares (claimable value) to a legally-sanctioned address is a compliance violation and an irreversible loss; the share recipient is a screenable destination. Same OFAC/EU/UN feed, new bind-site (TransferShares.recipient). (OFAC SDN list / EU / UN sanctions.)
// bucket: compliance/sanctions
// tags: bucket:compliance cat:sanctions enrichment method:address.sanctions could-be-compliance:true action-family:transfer
// methods: address.sanctions -> ../../../_methods/address.sanctions.md
// dormant: until address.sanctions is registered in method-catalog.json
//
// `address.sanctions` screens the share-transfer recipient against OFAC/EU/UN lists.
// ENRICHMENT — dormant until the dispatcher serves the method; the
// `context.custom has sanctioned` guard is false (inert) until then.
@id("sanctioned-liquid-staking-recipient-deny")
@severity("deny")
@reason("Liquid-staking share recipient is on a sanctions list (OFAC/EU/UN) — transfer blocked")
forbid(principal, action == LiquidStaking::Action::"TransferShares", resource)
when {
  context has custom
  && context.custom has sanctioned
  && context.custom.sanctioned == true
};

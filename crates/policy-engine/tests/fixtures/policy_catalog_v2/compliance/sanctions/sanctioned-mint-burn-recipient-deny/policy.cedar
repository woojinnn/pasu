// blocks: a token mint/bridge-out whose destination recipient is on an OFAC SDN / EU / UN sanctions list.
// why:    sanctions screening — mint/burn-and-bridge flows deliver fresh balance to a destination; when that destination is sanctioned, value is issued to a blocked party. The destination is modeled as the transfer recipient slot (the mint/bridge leg decodes to an Erc20Transfer to the destination). (OFAC SLS / Treasury SDN list.)
// bucket: compliance/sanctions
// tags: bucket:compliance cat:sanctions enrichment method:address.sanctions could-be-compliance:true action-family:transfer
// methods: address.sanctions -> ../../../_methods/address.sanctions.md
// dormant: until address.sanctions is registered in method-catalog.json
//
// `address.sanctions` screens the mint/bridge destination (transfer recipient
// slot) against OFAC/EU/UN lists. ENRICHMENT — dormant until the method is wired.
@id("sanctioned-mint-burn-recipient-deny")
@severity("deny")
@reason("Mint/bridge destination recipient is on a sanctions list (OFAC/EU/UN) — blocked")
forbid(principal, action == Token::Action::"Erc20Transfer", resource)
when {
  context has custom
  && context.custom has sanctioned
  && context.custom.sanctioned == true
};

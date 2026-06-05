// blocks: an ERC-20 transfer whose recipient is on an OFAC SDN / EU / UN sanctions list.
// why:    sanctions screening — sending value to a legally-sanctioned address is a compliance violation and an irreversible loss; a sanctions feed is the wallet-side analog of a VASP screen (OFAC SLS / Treasury SDN list).
// bucket: compliance/sanctions
// tags: bucket:compliance cat:sanctions enrichment method:address.sanctions could-be-compliance:true action-family:transfer
// methods: address.sanctions -> ../../../_methods/address.sanctions.md
// dormant: until address.sanctions is registered in method-catalog.json
//
// `address.sanctions` screens the transfer recipient against OFAC/EU/UN lists.
// ENRICHMENT — dormant until the dispatcher serves the method; the
// `context.custom has sanctioned` guard is false (inert) until then.
@id("sanctioned-recipient-deny")
@severity("deny")
@reason("Recipient is on a sanctions list (OFAC/EU/UN) — transfer blocked")
forbid(principal, action == Token::Action::"Erc20Transfer", resource)
when {
  context has custom
  && context.custom has sanctioned
  && context.custom.sanctioned == true
};

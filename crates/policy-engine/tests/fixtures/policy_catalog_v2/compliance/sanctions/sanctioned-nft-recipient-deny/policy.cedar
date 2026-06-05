// blocks: an NFT (ERC-721/1155) transfer whose recipient is on an OFAC SDN / EU / UN sanctions list.
// why:    sanctions screening — moving an asset to a legally-sanctioned address is a compliance violation and an irreversible loss; an NFT transfer is just as screenable as an ERC-20 value transfer. Same OFAC SLS / Treasury SDN feed, new bind-site (NftTransfer.recipient). (OFAC SDN list / EU / UN sanctions.)
// bucket: compliance/sanctions
// tags: bucket:compliance cat:sanctions enrichment method:address.sanctions could-be-compliance:true action-family:transfer
// methods: address.sanctions -> ../../../_methods/address.sanctions.md
// dormant: until address.sanctions is registered in method-catalog.json
//
// `address.sanctions` screens the NFT-transfer recipient against OFAC/EU/UN lists.
// ENRICHMENT — dormant until the dispatcher serves the method; the
// `context.custom has sanctioned` guard is false (inert) until then.
@id("sanctioned-nft-recipient-deny")
@severity("deny")
@reason("NFT recipient is on a sanctions list (OFAC/EU/UN) — transfer blocked")
forbid(principal, action == Token::Action::"NftTransfer", resource)
when {
  context has custom
  && context.custom has sanctioned
  && context.custom.sanctioned == true
};

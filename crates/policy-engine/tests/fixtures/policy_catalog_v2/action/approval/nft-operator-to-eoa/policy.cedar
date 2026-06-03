// blocks: granting collection-wide NFT operator rights (setApprovalForAll true) to an EOA rather than a marketplace contract.
// why:    operator=EOA pull — legitimate NFT operators (marketplaces) are contracts; granting collection-wide rights to a person-controlled key is a classic phishing setup that lets the operator sweep the whole collection (Pocket Universe approval-drain class).
// bucket: action/approval
// tags: bucket:action cat:approval enrichment method:address.activity action-family:approval
// methods: address.activity -> ../../../_methods/address.activity.md
// dormant: until address.activity is registered in method-catalog.json
//
// Warn only when GRANTING (approved == true) to an EOA (isContract == false);
// revokes pass freely. ENRICHMENT — dormant until the dispatcher serves it.
@id("nft-operator-to-eoa")
@severity("warn")
@reason("Operator is a regular wallet (EOA), not a marketplace contract — verify before granting collection-wide rights")
forbid(principal, action == Token::Action::"NftSetApprovalForAll", resource)
when {
  context.approved == true
  && context has custom
  && context.custom has isContract
  && context.custom.isContract == false
};

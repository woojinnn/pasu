// blocks: granting collection-wide NFT operator rights (setApprovalForAll true) to a flagged drainer / scam operator.
// why:    full-collection NFT drain — one setApprovalForAll grant hands an operator transfer rights over the owner's entire collection; granting it to a known-malicious operator is the top NFT-phishing loss vector (Pocket Universe / OpenSea-Blur listing scams).
// bucket: action/approval
// tags: bucket:action cat:approval enrichment method:address.reputation could-be-compliance:true action-family:approval
// methods: address.reputation -> ../../../_methods/address.reputation.md
// dormant: until address.reputation is registered in method-catalog.json
//
// Deny only when GRANTING (approved == true) to a flagged operator; revokes
// pass freely. ENRICHMENT — dormant until the dispatcher serves the method.
@id("nft-operator-reputation")
@severity("deny")
@reason("Operator is flagged as a known-malicious address — collection-wide approval blocked")
forbid(principal, action == Token::Action::"NftSetApprovalForAll", resource)
when {
  context.approved == true
  && context has custom
  && context.custom has reputationFlagged
  && context.custom.reputationFlagged == true
};

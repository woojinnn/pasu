// blocks: granting collection-wide operator rights via setApprovalForAll(operator, true) without an explicit confirm
// why:    NFT setApprovalForAll drainer — one signature hands an operator transfer rights over the owner's entire ERC-721 collection (current + future), the top NFT-phishing vector
// bucket: action/approval  (precedence-won)
// tags: bucket:action cat:approval static
// Catalog A3 — confirm before granting collection-wide NFT operator rights.
//
// `setApprovalForAll(operator, true)` hands one address transfer rights over the
// caller's ENTIRE collection (current + future) — a top NFT-drainer vector. Warn
// only when GRANTING (`context.approved == true`); revokes (`false`) pass freely.
// Pure-static. Threat refs: RareSkills ERC-721; OpenSea/Blur listing scams.
@id("nft-set-approval-for-all-confirm")
@severity("warn")
@reason("Granting operator rights over your entire NFT collection — confirm the operator is a marketplace you trust")
forbid(principal, action == Token::Action::"NftSetApprovalForAll", resource)
when {
  context.approved == true
};

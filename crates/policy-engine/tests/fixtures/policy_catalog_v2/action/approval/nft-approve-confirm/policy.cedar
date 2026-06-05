// blocks: signing an ERC-721 approve(spender, tokenId) without an explicit confirm step
// why:    NFT approval phishing — a malicious dApp tricks the owner into granting transfer rights over a high-value token (ERC-721 approve), a known NFT-drainer vector
// bucket: action/approval  (precedence-won)
// tags: bucket:action cat:approval static
// Catalog A4 — confirm a single-NFT operator approval.
//
// `approve(spender, tokenId)` grants transfer rights over one specific NFT. Less
// dangerous than setApprovalForAll but still a phishing surface for high-value
// tokens, so confirm unconditionally (the spender is statically decoded). Pure-static.
@id("nft-approve-confirm")
@severity("warn")
@reason("Approving an NFT for transfer by another address — confirm the spender before signing")
forbid(principal, action == Token::Action::"NftApprove", resource);

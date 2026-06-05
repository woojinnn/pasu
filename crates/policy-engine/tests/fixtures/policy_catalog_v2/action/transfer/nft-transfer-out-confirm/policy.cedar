// blocks: an NFT transfer that moves the token to an address other than the sender, without an explicit confirm step.
// why:    irreversible NFT move — `transferFrom`/`safeTransferFrom` of an NFT to a third party is unrecoverable; drainers and phishing flows route NFTs to attacker addresses, so any outbound move deserves a deliberate confirm. Source: plan §2A transfer / EIP-721.
// bucket: action/transfer
// tags: bucket:action cat:transfer static action-family:nft
// Catalog (action/transfer) — warn when an NFT is transferred to anyone other than
// the signer. NftTransfer carries `recipient:String`; `meta.submitter` is the
// actor. Both are lowercase 0x. Pure-static.
@id("nft-transfer-out-confirm")
@severity("warn")
@reason("Transferring this NFT to another address — confirm the recipient before signing (irreversible)")
forbid(principal, action == Token::Action::"NftTransfer", resource)
when {
  context.recipient != context.meta.submitter
};

// blocks: an NFT (ERC-721/1155) transfer to a recipient not on the user's allowlist
// why:    NFT moves are irreversible and high-value; confining the destination to a vetted allowlist stops a wrong/poisoned address or a scam "transfer your NFT here" lure (Rabby whitelist model)
// bucket: wallet/recipient-allowlist
// tags: bucket:wallet cat:recipient-allowlist static action-family:recipient-allowlist
// User-configured allowlist (lowercase 0x; placeholders below). Pure-static.
@id("nft-recipient-allowlist")
@severity("deny")
@reason("NFT recipient is not on your allowlist — transfer blocked")
forbid(principal, action == Token::Action::"NftTransfer", resource)
unless {
  ["0x000000000000000000000000000000000000dead", "0x1111111111111111111111111111111111111111"].contains(context.recipient)
};

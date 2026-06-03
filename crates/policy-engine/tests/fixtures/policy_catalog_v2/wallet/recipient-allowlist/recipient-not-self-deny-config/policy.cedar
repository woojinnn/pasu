// blocks: an ERC-20 transfer to any address other than the user's own owned/self set
// why:    a self-only config (Rabby) restricts outbound transfers to wallets the user controls — any external recipient is blocked, defeating drainer sweeps and mistaken sends
// bucket: wallet/recipient-allowlist
// tags: bucket:wallet cat:recipient-allowlist static action-family:recipient-allowlist
// Owned-address set is user-configured (lowercase 0x; placeholders below). Pure-static.
@id("recipient-not-self-deny-config")
@severity("deny")
@reason("Recipient is not one of your own addresses — self-only transfer policy blocks this")
forbid(principal, action == Token::Action::"Erc20Transfer", resource)
unless {
  ["0x1111111111111111111111111111111111111111", "0x2222222222222222222222222222222222222222"].contains(context.recipient)
};

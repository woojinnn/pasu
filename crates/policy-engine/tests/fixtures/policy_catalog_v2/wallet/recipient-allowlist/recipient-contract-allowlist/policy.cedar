// blocks: an ERC-20 transfer to any address not on the user's strict spending allowlist
// why:    a hard allowlist (deny-tier) confines outbound funds to vetted destinations — fat-finger, address-poisoning, and compromised-dApp redirects all fail closed (Rabby whitelist model)
// bucket: wallet/recipient-allowlist
// tags: bucket:wallet cat:recipient-allowlist static action-family:recipient-allowlist
// User-configured allowlist (lowercase 0x; placeholders below). Pure-static.
@id("recipient-contract-allowlist")
@severity("deny")
@reason("Recipient is not on your spending allowlist — transfer blocked")
forbid(principal, action == Token::Action::"Erc20Transfer", resource)
unless {
  ["0x000000000000000000000000000000000000dead", "0x1111111111111111111111111111111111111111", "0x2222222222222222222222222222222222222222"].contains(context.recipient)
};

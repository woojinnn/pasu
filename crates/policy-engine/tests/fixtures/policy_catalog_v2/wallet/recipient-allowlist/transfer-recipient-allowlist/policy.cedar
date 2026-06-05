// blocks: an ERC-20 transfer to a recipient not on the user's allowlist
// why:    a wrong/poisoned destination address sends tokens irreversibly; confining outbound transfers to a known-good allowlist catches fat-finger and address-poisoning sends before signing (address-poisoning threat class)
// bucket: wallet/recipient-allowlist  (precedence-won; cross-cutting attrs preserved as tags)
// tags: bucket:wallet cat:recipient-allowlist static action-family:recipient-allowlist
// Catalog D1 — warn on an ERC-20 transfer to a non-allowlisted recipient.
// User-configured allowlist (lowercase 0x; placeholders below). Pure-static.
@id("transfer-recipient-allowlist")
@severity("warn")
@reason("Transfer to an address outside your allowlist — double-check the recipient")
forbid(principal, action == Token::Action::"Erc20Transfer", resource)
when {
  !(["0x000000000000000000000000000000000000dead", "0x1111111111111111111111111111111111111111"].contains(context.recipient))
};

// blocks: a Permit2 transferFrom whose recipient is not on the user's allowlist
// why:    Permit2 (Uniswap permit2) lets an approved spender pull tokens to ANY recipient; an allowlist confines the destination so a malicious/compromised spender cannot redirect funds (token-drain / address-poisoning threat class)
// bucket: wallet/recipient-allowlist  (precedence-won; cross-cutting attrs preserved as tags)
// tags: bucket:wallet cat:recipient-allowlist static venue:permit2 action-family:recipient-allowlist
// Catalog B6 — restrict Permit2 transferFrom to an allowlisted recipient.
// The allowlist Set is user-configured (lowercase 0x addresses; placeholders
// below). Recipient is statically decoded. Pure-static.
@id("permit2-transferfrom-recipient-allowlist")
@severity("warn")
@reason("Permit2 transfer to a recipient outside your allowlist — confirm the destination")
forbid(principal, action == Token::Action::"Permit2TransferFrom", resource)
when {
  !(["0x000000000000000000000000000000000000dead", "0x1111111111111111111111111111111111111111"].contains(context.recipient))
};

// blocks: a swap whose output is routed to a recipient not on the user's allowlist
// why:    many routers let the swap output `recipient` differ from the sender; confining it to an allowlist stops output being silently diverted to an attacker (fund-redirection / address-poisoning threat class)
// bucket: wallet/recipient-allowlist  (precedence-won; cross-cutting attrs preserved as tags)
// tags: bucket:wallet cat:recipient-allowlist static action-family:recipient-allowlist
// Catalog C7 — warn when swap output is routed to a non-allowlisted address.
// Complements swap-recipient-not-self with an explicit destination allowlist
// (user-configured, lowercase 0x). Pure-static.
@id("swap-recipient-allowlist")
@severity("warn")
@reason("Swap output goes to an address outside your allowlist — confirm the recipient")
forbid(principal, action == Amm::Action::"Swap", resource)
when {
  !(["0x000000000000000000000000000000000000dead", "0x1111111111111111111111111111111111111111"].contains(context.recipient))
};

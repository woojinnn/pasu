// blocks: an ERC-20 transfer to a recipient already flagged as known-malicious (scam / drainer).
// why:    sending to a known-bad address is direct, irreversible loss; a reputation feed is the wallet-side analog of a sanctions/scam denylist (malicious-recipient threat class).
// bucket: action/transfer  (precedence-won; cross-cutting attrs preserved as tags)
// tags: bucket:action cat:transfer enrichment method:address.reputation could-be-compliance:sanctions-feed
// methods: address.reputation -> ../../../_methods/address.reputation.md
// Catalog D4 — block an ERC-20 transfer to a flagged recipient.
// `address.reputation` enriches whether the recipient is a known scam / drainer
// address. ENRICHMENT — dormant until the dispatcher serves `address.reputation`.
@id("transfer-recipient-reputation")
@severity("deny")
@reason("Recipient is flagged as a known-malicious address — transfer blocked")
forbid(principal, action == Token::Action::"Erc20Transfer", resource)
when {
  context has custom
  && context.custom has recipientFlagged
  && context.custom.recipientFlagged == true
};

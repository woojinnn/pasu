// blocks: an ERC-20 transfer to a never-before-seen recipient (zero tx history) slipping through unflagged.
// why:    a zero-activity recipient is a classic address-poisoning / typo signal — funds sent to a lookalike address are unrecoverable (address-poisoning threat class).
// bucket: action/transfer  (precedence-won; cross-cutting attrs preserved as tags)
// tags: bucket:action cat:transfer enrichment method:address.activity wallet-ux-spirit
// methods: address.activity -> ../../../_methods/address.activity.md
// Catalog D5 — warn on a transfer to a brand-new recipient (zero tx history).
// A recipient with no on-chain activity is often a typo or a freshly-generated
// address-poisoning lookalike. `address.activity` enriches the tx count.
// ENRICHMENT — dormant until the dispatcher serves `address.activity`.
@id("transfer-new-recipient")
@severity("warn")
@reason("Recipient has no transaction history — verify you have the right address (possible typo or poisoning)")
forbid(principal, action == Token::Action::"Erc20Transfer", resource)
when {
  context has custom
  && context.custom has recipientTxCount
  && context.custom.recipientTxCount == 0
};

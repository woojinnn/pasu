// blocks: an ERC-20 transfer to a brand-new recipient (zero outbound history, first seen inside the cooldown window).
// why:    Safe{Wallet} Guardrail-style new-counterparty cooldown — a never-used, just-observed recipient is the classic typo / address-poisoning / fresh-drainer signal; a short grace window forces a second look (new-recipient threat class).
// bucket: wallet/cooldown
// tags: bucket:wallet cat:cooldown enrichment method:address.activity action-family:cooldown
// methods: address.activity -> ../../../_methods/address.activity.md
// dormant: until address.activity is registered in method-catalog.json
//
// Cooldown — warn when sending to a recipient with no tx history that was first
// seen within the last 7 days (604800s). `address.activity` enriches both the
// recipient nonce and its first-seen timestamp. ENRICHMENT — dormant.
@id("new-recipient-cooldown")
@severity("warn")
@reason("Recipient is brand-new (no history, first seen recently) — wait out the cooldown and re-verify the address")
forbid(principal, action == Token::Action::"Erc20Transfer", resource)
when {
  context has custom
  && context.custom has recipientTxCount
  && context.custom.recipientTxCount == 0
  && context.custom has recipientFirstSeenTs
  && context.custom.recipientFirstSeenTs > (context.meta.submittedAt - 604800)
};

// blocks: an ERC-20 transfer to a counterparty younger than a minimum on-chain age (first seen within 30 days).
// why:    Safe{Wallet} Guardrail-style minimum-counterparty-age control — funds should only flow to addresses with an established history; a too-young address is a fresh drainer / poisoning lookalike (new-counterparty threat class).
// bucket: wallet/cooldown
// tags: bucket:wallet cat:cooldown enrichment method:address.activity action-family:cooldown
// methods: address.activity -> ../../../_methods/address.activity.md
// dormant: until address.activity is registered in method-catalog.json
//
// Cooldown — warn when the recipient was first seen within the last 30 days
// (2592000s). `address.activity` enriches the first-seen timestamp.
// ENRICHMENT — dormant.
@id("min-counterparty-age")
@severity("warn")
@reason("Recipient address is newer than your minimum-age threshold — confirm it is the intended counterparty")
forbid(principal, action == Token::Action::"Erc20Transfer", resource)
when {
  context has custom
  && context.custom has recipientFirstSeenTs
  && context.custom.recipientFirstSeenTs > (context.meta.submittedAt - 2592000)
};

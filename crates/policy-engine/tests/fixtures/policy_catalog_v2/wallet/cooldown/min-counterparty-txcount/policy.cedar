// blocks: an ERC-20 transfer to a recipient with fewer outbound transactions than a minimum-activity floor.
// why:    Safe{Wallet} Guardrail-style minimum-counterparty-activity control — a near-zero-nonce address is barely-used and a common drainer / poisoning signal; requiring a minimum tx count favours established counterparties (low-activity recipient threat class).
// bucket: wallet/cooldown
// tags: bucket:wallet cat:cooldown enrichment method:address.activity action-family:cooldown
// methods: address.activity -> ../../../_methods/address.activity.md
// dormant: until address.activity is registered in method-catalog.json
//
// Cooldown — warn when the recipient's outbound tx count is below the floor (5).
// `address.activity` enriches the recipient nonce. ENRICHMENT — dormant.
@id("min-counterparty-txcount")
@severity("warn")
@reason("Recipient has very little transaction history — confirm it is an established, intended counterparty")
forbid(principal, action == Token::Action::"Erc20Transfer", resource)
when {
  context has custom
  && context.custom has recipientTxCount
  && context.custom.recipientTxCount < 5
};

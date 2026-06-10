// blocks: a Hyperliquid spotSend whose destination is already flagged as known-malicious (scam / drainer).
// why:    sending spot funds off the venue to a known-bad address is direct, irreversible loss; a reputation feed is the wallet-side analog of a sanctions/scam denylist (malicious-recipient threat class). (hl docs — spotSend)
// bucket: protocol/hyperliquid  (precedence-won)
// tags: bucket:protocol cat:hyperliquid enrichment method:address.reputation venue:hyperliquid could-be-compliance:true action-family:hl-transfer
// methods: address.reputation -> ../../../_methods/address.reputation.md
//
// Catalog — block a Hyperliquid spotSend to a flagged destination.
// `address.reputation` (selector destination) enriches whether the destination is
// a known scam / drainer. ENRICHMENT — dormant until the dispatcher serves
// `address.reputation`; guard presence so the policy is inert until then.
// dormant: until address.reputation registered
@id("hl-spot-send-recipient-reputation")
@severity("deny")
@reason("spotSend destination is flagged as a known-malicious address — send blocked")
forbid(principal, action == Token::Action::"Erc20Transfer", resource)
when {
  context has custom
  && context.custom has reputationFlagged
  && context.custom.reputationFlagged == true
};

// blocks: signing a CowSwap (intent) order whose receiver is a known-malicious (scam / drainer) address
// why:    malicious-recipient on a signed intent — a solver-fillable order that delivers the buy-side tokens to a flagged address is direct, irreversible loss; a reputation feed is the wallet-side analog of a scam denylist (CoW Protocol order docs)
// bucket: protocol/intent-venue
// tags: bucket:protocol cat:intent-venue enrichment method:address.reputation venue:cow_swap could-be-compliance:true action-family:recipient-reputation
// methods: address.reputation -> ../../../_methods/address.reputation.md
// dormant: until address.reputation is registered in method-catalog.json
@id("cowswap-order-receiver-reputation-deny")
@severity("deny")
@reason("This intent order's receiver is flagged as a known-malicious address — order blocked")
forbid(principal, action == Amm::Action::"SignIntentOrder", resource)
when {
  context has custom
  && context.custom has receiverFlagged
  && context.custom.receiverFlagged == true
};

// blocks: a swap routed through a venue/router contract flagged as a known-malicious or fake aggregator.
// why:    fake-aggregator threat class — spoofed router/aggregator contracts steal approved input or mis-route swaps; a reputation feed on the router is the wallet-side denylist (source: blockaid).
// bucket: action/swap
// tags: bucket:action cat:swap enrichment method:address.reputation action-family:swap could-be-compliance:true
// methods: address.reputation -> ../../../_methods/address.reputation.md
// dormant: until address.reputation is registered in method-catalog.json
// address.reputation enriches whether the swap venue's router contract is
// known-malicious. ENRICHMENT — dormant until served.
@id("swap-venue-reputation")
@severity("deny")
@reason("This swap routes through a router flagged as a fake / malicious aggregator — blocked")
forbid(principal, action == Amm::Action::"Swap", resource)
when {
  context has custom
  && context.custom has reputationFlagged
  && context.custom.reputationFlagged == true
};

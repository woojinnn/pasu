// blocks: a swap whose output token contract is flagged as a known scam / malicious token by a reputation feed.
// why:    scam-token threat class — buying a flagged token converts good funds into a worthless/trap asset; a reputation feed is the wallet-side scam denylist (source: MetaMask).
// bucket: action/swap
// tags: bucket:action cat:swap enrichment method:address.reputation action-family:swap could-be-compliance:true
// methods: address.reputation -> ../../../_methods/address.reputation.md
// dormant: until address.reputation is registered in method-catalog.json
// address.reputation enriches whether the bought token contract (tokenOut
// address) is known-malicious. ENRICHMENT — dormant until served.
@id("swap-output-token-reputation")
@severity("warn")
@reason("The token you are buying is flagged as a known scam token — confirm before proceeding")
forbid(principal, action == Amm::Action::"Swap", resource)
when {
  context has custom
  && context.custom has reputationFlagged
  && context.custom.reputationFlagged == true
};

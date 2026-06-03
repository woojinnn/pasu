// blocks: signing a marketplace order to buy into an NFT collection that is flagged as a known scam / drainer collection.
// why:    scam-collection bait — fraudulent mints and wash-traded collections lure buyers via off-chain orders; a reputation feed on the bought collection's contract flags the order before the signer commits funds. Source: plan §2A nft-order / MetaMask scam-collection alerts.
// bucket: action/nft
// tags: bucket:action cat:nft enrichment method:address.reputation action-family:nft could-be-compliance:sanctions-feed
// methods: address.reputation -> ../../../_methods/address.reputation.md
// Catalog (action/nft) — warn when the bought collection is flagged malicious.
// SignIntentOrder's `buy:TokenRef` is the acquired asset; the method selector
// binds `buy.key.contract` (the NFT collection address). `address.reputation`
// enriches `reputationFlagged`. ENRICHMENT — dormant until the dispatcher serves it.
// dormant: until address.reputation is registered in method-catalog.json
@id("nft-collection-reputation")
@severity("warn")
@reason("This NFT collection is flagged as a known scam — review before signing the order")
forbid(principal, action == Amm::Action::"SignIntentOrder", resource)
when {
  context has custom
  && context.custom has reputationFlagged
  && context.custom.reputationFlagged == true
};

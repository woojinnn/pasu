// blocks: a swap that buys an unverified output token (no verified-contract metadata), a hallmark of honeypot / counterfeit-token bait.
// why:    honeypot bait / unverified-token threat class — scam tokens lure buys via fake routes; an unverified output contract is a leading red flag (source: blockaid).
// bucket: action/swap
// tags: bucket:action cat:swap enrichment method:token.metadata action-family:swap
// methods: token.metadata -> ../../../_methods/token.metadata.md
// dormant: until token.metadata is registered in method-catalog.json
// token.metadata enriches whether the BOUGHT token (tokenOut) is a verified
// contract. ENRICHMENT — dormant until the dispatcher serves `token.metadata`.
@id("swap-buy-unverified-token")
@severity("warn")
@reason("This swap buys an unverified token — it may be a honeypot or counterfeit; confirm before proceeding")
forbid(principal, action == Amm::Action::"Swap", resource)
when {
  context has custom
  && context.custom has isVerified
  && context.custom.isVerified == false
};

// blocks: signing a Pendle limit order whose expiry sits far in the future, leaving a standing maker-fillable order long after the market has moved
// why:    stale-TTL signed-order risk — a Pendle off-chain limit order (sign_limit_order) is a maker mandate a filler can exercise later at a price that has since drifted against the signer (Pendle V2 limit-order docs)
// bucket: protocol/pendle
// tags: bucket:protocol cat:pendle enrichment method:clock.now venue:pendle action-family:stale-ttl
// methods: clock.now -> ../../../_methods/clock.now.md
// dormant: until clock.now is registered in method-catalog.json
// NOTE: VOCAB §4 yields `expiry:String` (U256 hex) on SignLimitOrder — Cedar cannot do a
//       magnitude compare on a hex String, so the TTL window (`expiry > nowTs + window`) is
//       NOT expressible until the host projects a Long sibling for `expiry`. Until then this
//       rule presence-guards `nowTs` only and stays inert (no Long expiry to test against).
@id("pendle-sign-limit-order-far-expiry")
@severity("warn")
@reason("This Pendle limit order has a far-future expiry — a long TTL risks a stale-price fill")
forbid(principal, action == Yield::Action::"SignLimitOrder", resource)
when {
  context has custom
  && context.custom has nowTs
};

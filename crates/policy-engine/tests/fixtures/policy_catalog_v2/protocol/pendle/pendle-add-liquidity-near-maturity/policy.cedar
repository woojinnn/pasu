// blocks: adding Pendle market liquidity when the market is close to (or past) its maturity, where LP yield is near-exhausted and impermanent-loss/illiquidity risk is highest
// why:    near-maturity LP trap — Pendle PT/YT markets converge as maturity approaches; adding liquidity then earns little and is hard to exit, a poor-outcome guard the user should confirm (Pendle V2 AMM/maturity docs)
// bucket: protocol/pendle
// tags: bucket:protocol cat:pendle enrichment method:clock.now venue:pendle action-family:near-maturity
// methods: clock.now -> ../../../_methods/clock.now.md
// dormant: until clock.now is registered in method-catalog.json
// NOTE: VOCAB §4 yields `maturity:String` (U256 hex) on AddMarketLiquidity — Cedar cannot do a
//       magnitude compare on a hex String, so the window test (`maturity - nowTs < window`) is
//       NOT expressible until the host projects a Long sibling for `maturity`. Until then this
//       rule presence-guards `nowTs` only and stays inert (no Long maturity to test against).
@id("pendle-add-liquidity-near-maturity")
@severity("warn")
@reason("This Pendle market is near maturity — adding liquidity now earns little yield and is hard to exit")
forbid(principal, action == Yield::Action::"AddMarketLiquidity", resource)
when {
  context has custom
  && context.custom has nowTs
};

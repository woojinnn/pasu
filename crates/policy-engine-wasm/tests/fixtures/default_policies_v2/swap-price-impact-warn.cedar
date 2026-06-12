// dambi default v2 policy — explorer id: UNI-03 (estimated price impact too
// large → thin pool, you move the market against yourself).
//
// `priceImpactBp` is a base `Long` field of `Amm::SwapContext` (a lowered
// `SwapLiveInputs.price_impact_bp` LiveField), so NO enrichment is required
// (sv = LI). Uniswap itself warns above 5%; this warns above 5% (500 bp). The
// dApp-supplied number can later be cross-checked against an oracle fair price
// for a more conservative value, but the projected LiveField is sufficient here.
@id("swap-price-impact-warn")
@severity("warn")
@reason("This swap's estimated price impact is high — you may be moving the pool against yourself")
forbid(principal, action == Amm::Action::"Swap", resource)
when { context.priceImpactBp > 500 };

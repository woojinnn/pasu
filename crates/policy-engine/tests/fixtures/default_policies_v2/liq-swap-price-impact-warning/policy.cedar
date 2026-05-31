// scopeball default v2 policy — explorer id: E3 / LIQ (swap price-impact guard).
//
// Personal-trading (TradFi pre-trade) family. Warn when an AMM swap's estimated
// price impact exceeds 3% (300 bp) — you are likely moving a thin pool against
// yourself. `priceImpactBp` is a base field of `Amm::SwapContext` (a lowered
// `LiveField<u32>`), so NO enrichment is required: this is among the cheapest
// personal-trading policies to ship (sv = COV/LI today, per tradfi mapping §E3).
@id("liq-swap-price-impact-warning")
@severity("warn")
@reason("This swap's estimated price impact is high — you may be moving the pool against yourself")
forbid(principal, action == Amm::Action::"Swap", resource)
when { context.priceImpactBp > 300 };

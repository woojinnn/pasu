// scopeball default v2 policy — explorer id: UNI-02 (slippage tolerance set far
// too wide → sandwich-MEV bait).
//
// A wide slippage band lets an MEV bot push the price out and back within that
// band (the canonical sandwich; a documented $215K loss). `slippageBp` is a
// base `Long` field of `Amm::SwapContext` (lowered from `SwapParams.slippage_bp`,
// or reducer-derived from min_out↔expected), so this judges with NO enrichment
// (sv = COV). 300 bp (3%) is the warn threshold here.
@id("swap-slippage-high-warn")
@severity("warn")
@reason("This swap allows a very wide slippage band — an easy target for sandwich MEV")
forbid(principal, action == Amm::Action::"Swap", resource)
when { context.slippageBp > 300 };

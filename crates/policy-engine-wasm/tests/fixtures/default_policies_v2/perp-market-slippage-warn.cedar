// dambi default v2 policy — explorer id: PERP-05 (market order with a loose
// slippage tolerance).
//
// On a thin book or through a liquidation band a market order fills far from
// intent (the classic alt-market fat-finger). The order's own tolerance is the
// signal: `slippageBp` is a base `Long` field of `Perp::OpenPositionContext`, so
// this reads `context.slippageBp` DIRECTLY — no enrichment, no method, no
// custom_context (like `swap-slippage-high-warn`). Warn when the allowed slippage
// is above 100 bp (1%, user config).
@id("perp-market-slippage-warn")
@severity("warn")
@reason("This market order allows a large slippage — it could fill far from the price you expect")
forbid(principal, action == Perp::Action::"OpenPosition", resource)
when { context.slippageBp > 100 };

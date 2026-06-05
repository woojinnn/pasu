// blocks: signing a Pendle PT swap whose minAmountOut == 0, leaving no slippage floor on the yield-token swap
// why:    unbounded-slippage / sandwich-MEV exposure — a zero min-out lets a frontrunner extract the entire output via a sandwich
// bucket: protocol/pendle  (precedence-won; cross-cutting attrs preserved as tags)
// tags: bucket:protocol cat:pendle static action-family:min-out-zero venue:pendle
// Catalog I6 — warn on a Pendle PT swap with zero minimum output.
// `minAmountOut == 0` removes the slippage floor on a yield-token swap.
// Pure-static (`minAmountOut` rendered lower-hex; zero is `0x0`).
@id("pt-swap-min-out-zero")
@severity("warn")
@reason("This PT swap sets no minimum output — add a slippage floor before signing")
forbid(principal, action == Yield::Action::"PtSwap", resource)
when { context.minAmountOut == "0x0" };

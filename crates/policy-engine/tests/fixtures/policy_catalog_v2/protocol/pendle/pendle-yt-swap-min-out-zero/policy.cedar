// blocks: signing a Pendle YT swap whose minAmountOut == 0, leaving no slippage floor on the yield-token swap
// why:    unbounded-slippage / sandwich-MEV exposure — a zero min-out lets a frontrunner extract the entire YT-swap output via a sandwich (Pendle V2 Router docs)
// bucket: protocol/pendle
// tags: bucket:protocol cat:pendle static venue:pendle action-family:min-out-zero
@id("pendle-yt-swap-min-out-zero")
@severity("warn")
@reason("This YT swap sets no minimum output — add a slippage floor before signing")
forbid(principal, action == Yield::Action::"YtSwap", resource)
when { context.minAmountOut == "0x0" };

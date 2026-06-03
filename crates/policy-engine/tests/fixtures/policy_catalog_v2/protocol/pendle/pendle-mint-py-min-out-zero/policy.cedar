// blocks: minting Pendle PT+YT (mint_py) with minPyOut == 0, leaving no floor on how much PT/YT the deposit must produce
// why:    unbounded-slippage exposure — a zero minPyOut lets the mint settle at any (worst-case) PT/YT output, a sandwich/MEV loss vector on the SY→PY conversion (Pendle V2 Router docs)
// bucket: protocol/pendle
// tags: bucket:protocol cat:pendle static venue:pendle action-family:min-out-zero
@id("pendle-mint-py-min-out-zero")
@severity("warn")
@reason("This PT/YT mint sets no minimum output — add a slippage floor before signing")
forbid(principal, action == Yield::Action::"MintPy", resource)
when { context.minPyOut == "0x0" };

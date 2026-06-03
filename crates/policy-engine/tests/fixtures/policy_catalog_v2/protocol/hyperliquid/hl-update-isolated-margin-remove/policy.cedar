// blocks: removing isolated margin from a Hyperliquid position (negative ntli) without confirming the market and amount.
// why:    position-risk control — updateIsolatedMargin with a negative ntli pulls collateral out of an isolated position, moving the liquidation price against you; a maliciously injected margin-removal can push a position to the brink of liquidation. Gate only the remove (negative) direction; adding margin is benign.
// bucket: protocol/hyperliquid  (precedence-won)
// tags: bucket:protocol cat:hyperliquid static action-family:hl-margin venue:hyperliquid
//
// Catalog — confirm removing isolated margin on Hyperliquid (hl docs).
// ntli is a signed decimal string; "-*" matches the negative (remove) case. Pure-static.
@id("hl-update-isolated-margin-remove")
@severity("warn")
@reason("Removing isolated margin on Hyperliquid — confirm the market and amount; this moves your liquidation price")
forbid(principal, action == HyperliquidCore::Action::"HlUpdateIsolatedMargin", resource)
when { context.ntli like "-*" };

// blocks: placing a TWAP order on Hyperliquid without an explicit confirm.
// why:    a TWAP slices size over many minutes and keeps executing unattended — an injected or mistaken TWAP drains size over time before the user notices; confirm the order before it is sent. (hl docs — twapOrder)
// bucket: protocol/hyperliquid  (precedence-won)
// tags: bucket:protocol cat:hyperliquid static action-family:hl-order
//
// Catalog — confirm a Hyperliquid TWAP order (twapOrder).
// HL numerics are decimal strings (size/minutes magnitude not comparable in Cedar),
// so this is an unconditional pure-static confirm on the action.
@id("hl-twap-order-confirm")
@severity("warn")
@reason("Placing a TWAP order on Hyperliquid — confirm the market, side and size before it executes over time")
forbid(principal, action == HyperliquidCore::Action::"HlTwapOrder", resource);

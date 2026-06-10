// blocks: placing a TWAP order on Hyperliquid without an explicit confirm.
// why:    a TWAP slices size over many minutes and keeps executing unattended — an injected or mistaken TWAP drains size over time before the user notices; confirm the order before it is sent. (hl docs — twapOrder)
// bucket: protocol/hyperliquid  (precedence-won)
// tags: bucket:protocol cat:hyperliquid static action-family:hl-order
//
// Catalog — confirm a Hyperliquid TWAP order. HL orders decode to the unified
// `Perp::PlaceOrder`; the TWAP sub-kind is `context.orderType.kind == "twap"`.
// HL-scoped via the venue guard. Pure-static confirm.
@id("hl-twap-order-confirm")
@severity("warn")
@reason("Placing a TWAP order on Hyperliquid — confirm the market, side and size before it executes over time")
forbid(principal, action == Perp::Action::"PlaceOrder", resource)
when { context.venue.name == "hyperliquid" && context.orderType.kind == "twap" };

// dambi default v2 policy — block opening NEW short perp exposure on Hyperliquid.
//
// Hyperliquid `/exchange` orders now decode to the generic `Perp::PlaceOrder`
// (orderType limit/stop/twap), so ONE action rule covers plain + TWAP shorts.
// `side == "short"` is the order DIRECTION (sell), which also covers reduce-only
// long-CLOSES; the `reduceOnly == false` guard blocks only NEW short exposure,
// never a position exit. HL-scoped via `context.venue.name == "hyperliquid"`.
@id("hl-no-short-perp")
@severity("deny")
@reason("Opening a new short perp on Hyperliquid is blocked by policy")
forbid(principal, action == Perp::Action::"PlaceOrder", resource)
when {
  context.venue.name == "hyperliquid"
    && context.side == "short"
    && context.reduceOnly == false
};

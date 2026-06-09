// blocks: opening a NEW short-side perp on Hyperliquid (an example user-set directional risk limit).
// why:    perp directional risk control — a new short carries unbounded upside loss. `side` is the order DIRECTION (sell), which also covers reduce-only long-CLOSES, so the guard also tests `reduceOnly == false` to block only new short exposure, never a position exit.
// bucket: protocol/hyperliquid  (precedence-won; cross-cutting attrs preserved as tags)
// tags: bucket:protocol cat:hyperliquid static action-family:perp-risk venue:hyperliquid
//
// Catalog F1 — block opening NEW short perp exposure on Hyperliquid (risk limit).
// HL `/exchange` orders decode to the generic `Perp::PlaceOrder` (orderType
// limit/stop/twap), so a single action rule covers plain + TWAP shorts.
// HL-scoped via `context.venue.name == "hyperliquid"`. Pure-static.
@id("hl-no-short-perp")
@severity("deny")
@reason("Opening a new short perp on Hyperliquid is blocked by your policy")
forbid(principal, action == Perp::Action::"PlaceOrder", resource)
when {
  context.venue.name == "hyperliquid"
    && context.side == "short"
    && context.reduceOnly == false
};

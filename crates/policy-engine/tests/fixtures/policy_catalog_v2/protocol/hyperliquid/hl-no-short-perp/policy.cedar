// blocks: opening a NEW short-side perp on Hyperliquid (an example user-set directional risk limit).
// why:    perp directional risk control — a new short carries unbounded upside loss. `side` is the order DIRECTION (sell), which also covers reduce-only long-CLOSES, so the guard also tests `positionEffect == "open"` to block only new short exposure, never a position exit.
// bucket: protocol/hyperliquid  (precedence-won; cross-cutting attrs preserved as tags)
// tags: bucket:protocol cat:hyperliquid static action-family:perp-risk venue:hyperliquid
//
// Catalog F1 — block opening NEW short perp exposure on Hyperliquid (risk limit).
// `side == "short"` is the order DIRECTION (sell), which also covers reduce-only
// long-CLOSES; the `positionEffect == "open"` guard keeps this from blocking a
// position exit (reduce-only ⇒ "reduce"). Demonstrates a venue + side + intent
// guard on a venue-order (off-chain) action. Pure-static.
@id("hl-no-short-perp")
@severity("deny")
@reason("Opening a new short perp on Hyperliquid is blocked by your policy")
forbid(
  principal,
  // Plain order + TWAP — a TWAP short must not bypass the control.
  action in [HyperliquidCore::Action::"HlOrder", HyperliquidCore::Action::"HlTwapOrder"],
  resource
)
when {
  context.venue.name == "hyperliquid"
    && context.side == "short"
    && context.positionEffect == "open"
};

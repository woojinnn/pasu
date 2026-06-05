@id("hl-no-short-perp")
@severity("deny")
@reason("Opening a new short perp on Hyperliquid is blocked by policy")
forbid(
  principal,
  // Covers BOTH a plain order and a TWAP order — a TWAP short is still a short,
  // so it must not bypass this control (both contexts carry venue/side/positionEffect).
  action in [HyperliquidCore::Action::"HlOrder", HyperliquidCore::Action::"HlTwapOrder"],
  resource
)
when {
  // `side == "short"` is the order DIRECTION (sell), which also covers
  // reduce-only long-CLOSES. Gate on `positionEffect == "open"` so this blocks
  // only NEW short exposure, never a position exit (reduce-only ⇒ "reduce").
  context.venue.name == "hyperliquid"
    && context.side == "short"
    && context.positionEffect == "open"
};

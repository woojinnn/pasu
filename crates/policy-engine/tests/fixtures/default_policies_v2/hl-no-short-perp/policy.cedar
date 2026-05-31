@id("hl-no-short-perp")
@severity("deny")
@reason("Short perp orders on Hyperliquid are blocked by policy")
forbid(
  principal,
  action == HyperliquidCore::Action::"HlOrder",
  resource
)
when {
  context.venue.name == "hyperliquid" && context.side == "short"
};

@id("hl-confirm-high-leverage")
@severity("warn")
@reason("Raising leverage above 20x on Hyperliquid — confirm before applying")
forbid(
  principal,
  action == HyperliquidCore::Action::"HlUpdateLeverage",
  resource
)
when {
  context.venue.name == "hyperliquid" && context.leverage > 20
};

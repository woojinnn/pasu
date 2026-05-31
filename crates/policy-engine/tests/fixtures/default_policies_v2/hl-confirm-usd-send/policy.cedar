@id("hl-confirm-usd-send")
@severity("warn")
@reason("Sending USDC to another account on Hyperliquid — confirm before sending")
forbid(
  principal,
  action == HyperliquidCore::Action::"HlUsdSend",
  resource
);

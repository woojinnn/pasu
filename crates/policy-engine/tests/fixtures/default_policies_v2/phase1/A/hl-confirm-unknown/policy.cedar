@id("hl-confirm-unknown")
@severity("warn")
@reason("Unrecognized Hyperliquid action — confirm before the request is sent")
forbid(
  principal,
  action == HyperliquidCore::Action::"HlUnknown",
  resource
);

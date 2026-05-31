@id("hl-confirm-withdraw")
@severity("warn")
@reason("Withdrawing funds off Hyperliquid — confirm before the request is sent")
forbid(
  principal,
  action == HyperliquidCore::Action::"HlWithdraw",
  resource
);

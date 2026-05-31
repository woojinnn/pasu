@id("hl-confirm-approve-agent")
@severity("warn")
@reason("Authorizing an agent wallet to trade on your behalf — confirm before approving")
forbid(
  principal,
  action == HyperliquidCore::Action::"HlApproveAgent",
  resource
);

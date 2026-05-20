@id("user/max-fee-bps-30")
@severity("deny")
@reason("Protocol fee exceeds 30 basis points")
forbid (
  principal,
  action == Action::"swap",
  resource
) when {
  context has feeBps &&
  context.feeBps > 30
};

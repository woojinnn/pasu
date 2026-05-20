@id("user/max-fee-bps-100")
@severity("deny")
@reason("swap pool fee must be <= 100 bps (1%)")
forbid (
  principal,
  action == Action::"swap",
  resource
) when {
  context has feeBps && context.feeBps > 100
};

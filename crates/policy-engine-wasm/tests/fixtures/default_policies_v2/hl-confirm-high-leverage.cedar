@id("hl-confirm-high-leverage")
@severity("warn")
@reason("Raising leverage above 20x on Hyperliquid — confirm before applying")
forbid(principal, action == Perp::Action::"ChangeLeverage", resource)
when {
  context.venue.name == "hyperliquid"
    && context.newLeverage.greaterThan(decimal("20.0"))
};

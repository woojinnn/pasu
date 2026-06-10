@id("hl-no-short-perp")
@severity("deny")
@reason("Short perp orders on Hyperliquid are blocked by policy")
forbid(principal, action == Perp::Action::"PlaceOrder", resource)
when {
  context.venue.name == "hyperliquid"
    && context.side == "short"
    && context.reduceOnly == false
};

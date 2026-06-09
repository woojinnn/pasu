// blocks: setting Hyperliquid perp leverage above 20x, where a small adverse price move liquidates the whole position.
// why:    perp liquidation risk — extreme leverage shrinks the liquidation margin so an ordinary move wipes the position; the guard reads the change-leverage action's `newLeverage`.
// bucket: protocol/hyperliquid  (precedence-won; cross-cutting attrs preserved as tags)
// tags: bucket:protocol cat:hyperliquid static action-family:perp-risk venue:hyperliquid
//
// Catalog F2 — warn when setting Hyperliquid leverage above 20x. HL
// updateLeverage decodes to the generic Perp::ChangeLeverage; `newLeverage` is a
// Cedar `decimal` (leverage fits the 4-dp precision), comparable via
// `.greaterThan`. HL-scoped via the venue guard. Pure-static.
@id("hl-high-leverage")
@severity("warn")
@reason("Leverage above 20x — a small adverse move can liquidate the position")
forbid(principal, action == Perp::Action::"ChangeLeverage", resource)
when {
  context.venue.name == "hyperliquid"
    && context.newLeverage.greaterThan(decimal("20.0"))
};

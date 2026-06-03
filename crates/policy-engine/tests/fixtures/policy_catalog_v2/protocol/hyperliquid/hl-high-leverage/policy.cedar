// blocks: setting Hyperliquid perp leverage above 20x, where a small adverse price move liquidates the whole position.
// why:    perp liquidation risk — extreme leverage shrinks the liquidation margin so an ordinary move wipes the position; the guard reads the updateLeverage action's base `leverage` field.
// bucket: protocol/hyperliquid  (precedence-won; cross-cutting attrs preserved as tags)
// tags: bucket:protocol cat:hyperliquid static action-family:perp-risk venue:hyperliquid
//
// Catalog F2 — warn when setting Hyperliquid leverage above 20x.
// One adverse move liquidates a high-leverage position. `leverage` is a base
// field (Long) on the updateLeverage action. Pure-static.
@id("hl-high-leverage")
@severity("warn")
@reason("Leverage above 20x — a small adverse move can liquidate the position")
forbid(principal, action == HyperliquidCore::Action::"HlUpdateLeverage", resource)
when { context.leverage > 20 };

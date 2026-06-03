// blocks: placing a short-side perp order on Hyperliquid (an example user-set directional risk limit).
// why:    perp directional risk control — a short carries unbounded upside loss; this venue + side guard lets a user forbid an entire trade direction on the venue-order (off-chain) path.
// bucket: protocol/hyperliquid  (precedence-won; cross-cutting attrs preserved as tags)
// tags: bucket:protocol cat:hyperliquid static action-family:perp-risk venue:hyperliquid
//
// Catalog F1 — block short perp orders on Hyperliquid (example risk limit).
// Demonstrates a venue + side guard on a venue-order (off-chain) action.
// Pure-static.
@id("hl-no-short-perp")
@severity("deny")
@reason("Short perp orders on Hyperliquid are blocked by your policy")
forbid(principal, action == HyperliquidCore::Action::"HlOrder", resource)
when { context.venue.name == "hyperliquid" && context.side == "short" };

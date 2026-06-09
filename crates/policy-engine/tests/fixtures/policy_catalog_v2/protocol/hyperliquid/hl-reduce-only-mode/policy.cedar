// blocks: opening or adding to ANY Hyperliquid position — only reduce-only (position-closing) orders pass.
// why:    a "reduce-only / de-risk" lockdown — while active the wallet may only SHRINK existing exposure (close/trim), never open or add. Useful for a wind-down mandate, a compromised-key containment posture, or a risk desk freezing new exposure. Derived purely from the order's reduce_only flag; no live position data.
// bucket: protocol/hyperliquid  (precedence-won; cross-cutting attrs preserved as tags)
// tags: bucket:protocol cat:hyperliquid static action-family:perp-risk venue:hyperliquid lockdown
//
// Catalog — Hyperliquid reduce-only lockdown. HL orders decode to the unified
// `Perp::PlaceOrder` (covers plain + TWAP), so `reduceOnly == false` blocks
// every position-OPENING order regardless of order kind, while letting
// reduce-only closes/trims through. HL-scoped via the venue guard. Pure-static.
// Opt-in: NOT a shipped default — auto-enforcing it would block all normal opens.
@id("hl-reduce-only-mode")
@severity("deny")
@reason("Reduce-only mode is active — only position-closing (reduce-only) orders are allowed")
forbid(principal, action == Perp::Action::"PlaceOrder", resource)
when { context.venue.name == "hyperliquid" && context.reduceOnly == false };

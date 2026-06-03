// blocks: opening a perp position whose accepted slippage exceeds 1% (slippageBp > 100), risking a poor entry fill.
// why:    perp entry slippage abuse / MEV sandwich on position open — a wide slippage tolerance lets the fill price be pushed against the trader.
// bucket: action/perp  (precedence-won; cross-cutting attrs preserved as tags)
// tags: bucket:action cat:perp static action-family:slippage action-family:perp-risk
// Catalog F3 — warn when opening a perp position with slippage tolerance above 1%.
// `slippageBp` is a base field (Long) on OpenPosition. Pure-static. (Leverage is
// a decimal string on this action, so the numeric guard here is on slippage.)
@id("perp-open-high-slippage")
@severity("warn")
@reason("Opening this perp position allows over 1% slippage — you may get a poor entry fill")
forbid(principal, action == Perp::Action::"OpenPosition", resource)
when { context.slippageBp > 100 };

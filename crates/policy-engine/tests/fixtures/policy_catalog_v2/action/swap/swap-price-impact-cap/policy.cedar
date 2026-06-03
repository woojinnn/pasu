// blocks: a swap whose estimated price impact exceeds 5% (500 bp), indicating an oversized trade against a thin pool
// why:    price-impact / thin-pool overpayment — a high-impact trade moves the pool price against itself and overpays, and is also a hallmark of low-liquidity pool manipulation
// bucket: action/swap  (precedence-won)
// tags: bucket:action cat:swap static
// Catalog C2 — warn when a swap's estimated price impact exceeds 5% (500 bp).
// High price impact means a thin pool / oversized trade. `priceImpactBp` is a
// base LiveField projection (Long), no enrichment.
@id("swap-price-impact-cap")
@severity("warn")
@reason("This swap moves the pool price by more than 5% — you are likely overpaying on a thin market")
forbid(principal, action == Amm::Action::"Swap", resource)
when { context.priceImpactBp > 500 };

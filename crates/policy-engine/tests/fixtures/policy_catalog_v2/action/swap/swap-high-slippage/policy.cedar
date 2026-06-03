// blocks: a swap whose slippage tolerance exceeds 1% (100 bp), leaving the trade open to value extraction
// why:    MEV sandwich attack — a wide slippage tolerance lets a searcher front-run/back-run the swap and skim the difference
// bucket: action/swap  (precedence-won)
// tags: bucket:action cat:swap static action-family:slippage
// Catalog C1 — warn on a swap with slippage tolerance above 1% (100 bp).
// High slippage invites MEV sandwich extraction. `slippageBp` is a base field
// (Long), no enrichment. Threat refs: Uniswap MEV-protection; CowSwap sandwich.
@id("swap-high-slippage")
@severity("warn")
@reason("Slippage tolerance above 1% — you may lose value to a sandwich attack")
forbid(principal, action == Amm::Action::"Swap", resource)
when { context.slippageBp > 100 };

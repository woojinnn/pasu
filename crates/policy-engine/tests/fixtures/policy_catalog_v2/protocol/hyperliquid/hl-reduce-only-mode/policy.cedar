// blocks: opening or adding to ANY Hyperliquid position — only reduce-only (position-closing) orders pass.
// why:    a "reduce-only / de-risk" lockdown — while active the wallet may only SHRINK existing exposure (close/trim), never open or add. Useful for a wind-down mandate, a compromised-key containment posture, or a risk desk freezing new exposure. Derived purely from the order's reduce_only flag (positionEffect); no live position data.
// bucket: protocol/hyperliquid  (precedence-won; cross-cutting attrs preserved as tags)
// tags: bucket:protocol cat:hyperliquid static action-family:perp-risk venue:hyperliquid lockdown
//
// Catalog — Hyperliquid reduce-only lockdown. `positionEffect == "open"` is the
// derived intent (reduce-only ⇒ "reduce"), so this blocks every position-OPENING
// order regardless of side, while letting reduce-only closes/trims through.
// Pure-static (no enrichment). Opt-in: NOT a shipped default — auto-enforcing it
// would block all normal opening trades.
@id("hl-reduce-only-mode")
@severity("deny")
@reason("Reduce-only mode is active — only position-closing (reduce-only) orders are allowed")
forbid(
  principal,
  // Plain order + TWAP — a TWAP open must not bypass the lockdown.
  action in [HyperliquidCore::Action::"HlOrder", HyperliquidCore::Action::"HlTwapOrder"],
  resource
)
when { context.positionEffect == "open" };

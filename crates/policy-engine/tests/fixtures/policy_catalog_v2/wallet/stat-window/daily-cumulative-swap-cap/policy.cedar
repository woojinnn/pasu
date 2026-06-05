// blocks: a swap that pushes the wallet's rolling-window cumulative swap volume
//         over the $25k/day cap.
// why:    swap-velocity control (Fireblocks TIMEFRAME limits) — repeated moderate
//         swaps add up to large MEV/slippage exposure and can signal a drain in
//         progress; a per-action rolling-window USD ceiling gates the aggregate.
//         `stat_window.swap_stats` aggregates the actor's 24h swap volume into
//         `context.custom.windowSwapUsd`.
// bucket: wallet/stat-window
// tags: bucket:wallet cat:stat-window enrichment method:stat_window.swap_stats action-family:velocity-cap
//
// stat_window.swap_stats is engine-REGISTERED (method-catalog.json) — not dormant.
// `action` param is pinned to "swap" so the host aggregates the swap window only.
@id("daily-cumulative-swap-cap")
@severity("warn")
@reason("This swap pushes your daily swap volume over $25k — confirm before signing")
forbid(principal, action == Amm::Action::"Swap", resource)
when {
  context has custom
  && context.custom has windowSwapUsd
  && context.custom.windowSwapUsd.greaterThan(decimal("25000.0000"))
};

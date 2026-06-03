// blocks: a transfer that pushes the wallet's rolling-window cumulative outflow
//         over the $10k/day cap, even when each single transfer is small.
// why:    velocity / structuring control (Fireblocks TIMEFRAME limits) — a per-tx
//         cap misses drain-by-a-thousand-cuts; a rolling-window USD ceiling on
//         total outflow catches the aggregate. `stat_window.snapshot` aggregates
//         the actor's recent outflow into `context.custom.windowOutflowUsd`.
// bucket: wallet/stat-window
// tags: bucket:wallet cat:stat-window enrichment method:stat_window.snapshot action-family:velocity-cap
//
// stat_window.snapshot is engine-REGISTERED (method-catalog.json) — not dormant.
// The host aggregates the 24h outflow and projects the USD total to the
// `windowOutflowUsd` leaf; the guard fires once the cumulative crosses $10k.
@id("daily-cumulative-transfer-cap")
@severity("warn")
@reason("This transfer pushes your daily outflow over $10k — confirm before signing")
forbid(principal, action == Token::Action::"Erc20Transfer", resource)
when {
  context has custom
  && context.custom has windowOutflowUsd
  && context.custom.windowOutflowUsd.greaterThan(decimal("10000.0000"))
};

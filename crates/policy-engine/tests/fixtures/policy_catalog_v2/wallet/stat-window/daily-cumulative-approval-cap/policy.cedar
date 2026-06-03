// blocks: granting an approval after the wallet's rolling-window cumulative
//         outflow has already crossed the $10k/day cap.
// why:    velocity control over the standing-allowance surface (Fireblocks
//         TIMEFRAME limits) — approvals are the largest deferred-drain vector, so
//         the same daily ceiling that gates outflow also gates new pull rights.
//         `stat_window.snapshot` aggregates the actor's recent outflow into
//         `context.custom.windowOutflowUsd`.
// bucket: wallet/stat-window
// tags: bucket:wallet cat:stat-window enrichment method:stat_window.snapshot action-family:velocity-cap
//
// stat_window.snapshot is engine-REGISTERED (method-catalog.json) — not dormant.
// Reuses the wallet-scoped outflow window (no per-tag amount sum exists for
// approvals); once cumulative outflow exceeds $10k the day's new approvals warn.
@id("daily-cumulative-approval-cap")
@severity("warn")
@reason("Your daily activity already exceeds $10k — confirm this new approval before signing")
forbid(principal, action == Token::Action::"Erc20Approve", resource)
when {
  context has custom
  && context.custom has windowOutflowUsd
  && context.custom.windowOutflowUsd.greaterThan(decimal("10000.0000"))
};

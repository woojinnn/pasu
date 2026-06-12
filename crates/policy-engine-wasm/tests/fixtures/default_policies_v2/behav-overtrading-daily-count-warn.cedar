// dambi default v2 policy — explorer id: F2 / ORDER (over-trading / churn).
// Personal-trading (TradFi pre-trade) family.
//
// Rule: warn when the wallet has already executed more than the user's daily
// swap budget in the trailing 24h window — churning racks up fees and mistakes.
//
// WHY a method is required:
//   the number of swaps the wallet did in the last 24h is CROSS-REQUEST history
//   (a sliding window over past actions), not a field of THIS action's context.
//   Cedar sees only the single request. The manifest declares
//   `order.swap_count_24h`, projecting the running count into the `Long`
//   `swapCount24h`. The turnover-ratio variant (volume ÷ portfolio total) needs a
//   whole-wallet fold for the denominator and is a separate enrichment (F2's GAP
//   half) — this fixture ships the count half, which is already a bundled fact.
//   Defined-not-wired → `optional` (fail-open), so the field is `has`-guarded.
//   `Long` compares with `>` directly. Budget 20/day is user config.
@id("behav-overtrading-daily-count-warn")
@severity("warn")
@reason("You've already traded a lot today — churning like this mostly burns fees")
forbid(principal, action == Amm::Action::"Swap", resource)
when {
  context has custom
  && context.custom has swapCount24h
  && context.custom.swapCount24h > 20
};

// scopeball default v2 policy — explorer id: CURVE-2 (remove_liquidity_one_coin
// imbalance penalty).
//
// DERIVED fact (NOT an action-field). Withdrawing an LP position entirely into
// ONE coin is convenient but, the scarcer that coin is in the pool, the larger
// the imbalance penalty versus a proportional (balanced) exit — users "get way
// less" without knowing why. Quantifying it needs the pool's CURRENT per-coin
// balances (`live_inputs.pool_state` PoolState::StableV1.balances — reducer-
// internal, NOT on RemoveLiquidityContext) against the burned LP amount; the
// withdrawal sizes in `params.lpAmount` / `params.minOut` are U256 HEX STRINGS
// Cedar cannot compare numerically. So the penalty fraction (one-coin output
// shortfall vs balanced, as a decimal) is derived: the manifest declares
// `curve.one_coin_penalty` (reducer-fold over pool balances + burn amount),
// projected into `context.custom.oneCoinPenalty` (decimal). Warn above θ =
// 0.0100 (1% worse than balanced). Defined-not-wired → `optional` (fail-open).
@id("curve-one-coin-withdraw-penalty-warn")
@severity("warn")
@reason("Withdrawing into a single coin pays a Curve imbalance penalty — you receive noticeably less than a balanced exit")
forbid(principal, action == Amm::Action::"RemoveLiquidity", resource)
when {
  context has custom
  && context.custom has oneCoinPenalty
  && context.custom.oneCoinPenalty.greaterThan(decimal("0.0100"))
};

// scopeball default v2 policy — explorer id: CURVE-1 (one-sided add_liquidity
// into a Curve stableswap → silent imbalance fee).
//
// DERIVED fact (NOT an action-field). Depositing mostly the coin the pool is
// already heavy in pays an imbalance fee (a virtual-price penalty) that users
// mistake for "slippage" and never trace. Judging it needs the pool's CURRENT
// per-coin balance ratio vs the deposit's per-coin ratio. The pool balances
// live in `live_inputs.pool_state` (PoolState::StableV1 { balances, a, fee_bp })
// which is reducer-internal and NOT surfaced on the AddLiquidityContext; and
// the deposit amounts in `params.tokens` are U256 HEX STRINGS Cedar cannot
// compare numerically. So the skew (deposit-ratio vs pool-ratio, as a decimal)
// must be derived: the manifest declares `curve.imbalance_skew` (reducer-fold
// over pool balances + deposit amounts), projected into
// `context.custom.imbalanceSkew` (decimal). Warn above θ = 0.2500 (25% skew).
// Defined-not-wired → `optional` (fail-open no-op). θ is user config.
@id("curve-imbalanced-add-skew-warn")
@severity("warn")
@reason("This deposit is heavily one-sided versus the pool — Curve charges an imbalance fee that quietly reduces your LP tokens")
forbid(principal, action == Amm::Action::"AddLiquidity", resource)
when {
  context has custom
  && context.custom has imbalanceSkew
  && context.custom.imbalanceSkew.greaterThan(decimal("0.2500"))
};

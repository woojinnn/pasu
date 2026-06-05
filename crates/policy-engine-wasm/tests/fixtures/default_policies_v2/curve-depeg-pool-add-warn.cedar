// scopeball default v2 policy — explorer id: CURVE-3 (adding into a Curve pool
// whose pegged assets are actively depegging, e.g. stETH/ETH, crvUSD).
//
// DERIVED composite (NOT an action-field). When one asset of a peg pool loses
// its peg, the pool composition lurches (e.g. 95% stETH / 5% ETH); depositing
// then loads you onto the failing asset, and the contract never blocks it. This
// is the explorer's "fragile band": a normally-safe pool that is dangerous only
// at a specific moment. Judging it needs (a) the live peg deviation BETWEEN the
// pool's two pegged assets — a pair deviation, not the single-token peg_ratio —
// and (b) whether the CURRENT pool composition is outside its normal band, both
// read from `live_inputs.pool_state` (StableV1.balances, reducer-internal) and
// the assets' peg refs. Neither is a comparable action-field, so the manifest
// declares the RECORD method `curve.pool_depeg_state`, projecting two fields:
// `pegDeviationBp` (Long) and `compositionOutOfBand` (Bool). Warn when the pair
// deviation exceeds θ = 50bp OR composition is out of band. Defined-not-wired →
// `optional` (fail-open no-op). θ is user config.
@id("curve-depeg-pool-add-warn")
@severity("warn")
@reason("This Curve pool is actively depegging — adding now loads you onto the failing asset at a bad ratio")
forbid(principal, action == Amm::Action::"AddLiquidity", resource)
when {
  context has custom
  && context.custom has pegDeviationBp
  && context.custom has compositionOutOfBand
  && (context.custom.pegDeviationBp > 50 || context.custom.compositionOutOfBand)
};

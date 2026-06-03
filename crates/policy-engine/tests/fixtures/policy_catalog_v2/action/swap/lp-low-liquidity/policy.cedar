// blocks: adding liquidity to a thinly-traded pool (< $10k 24h volume) where the LP position is hard to exit and exposed to high impermanent loss
// why:    low-liquidity / illiquid-pool LP risk — thin pools can trap deposited capital and amplify impermanent loss and exit slippage
// bucket: action/swap  (precedence-won)
// tags: bucket:action cat:swap enrichment method:pool.liquidity
// methods: pool.liquidity -> ../../../_methods/pool.liquidity.md
// Catalog I4 — warn on providing liquidity to a thinly-traded pool (< $10k 24h volume).
// Low-volume pools mean illiquid LP and high impermanent-loss / exit risk.
// `pool.liquidity` enriches 24h volume. ENRICHMENT — dormant until served.
@id("lp-low-liquidity")
@severity("warn")
@reason("This pool has very low 24h volume — your LP may be hard to exit and exposed to impermanent loss")
forbid(principal, action == Amm::Action::"AddLiquidity", resource)
when {
  context has custom
  && context.custom has poolVol24hUsd
  && context.custom.poolVol24hUsd.lessThan(decimal("10000.0000"))
};

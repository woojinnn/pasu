// blocks: adding liquidity to an AMM pool above the $50k USD cap (lead-leg notional).
// why:    deposit-sizing risk — a large LP add concentrates funds in one pool's
//         impermanent-loss / hook / depeg surface; a USD ceiling forces a size
//         confirmation. source: Fireblocks transaction-authorization USD limits.
// bucket: wallet/usd-cap
// tags: bucket:wallet cat:usd-cap enrichment method:oracle.usd_value action-family:usd-cap
// methods: oracle.usd_value -> ../../../_methods/oracle.usd_value.md
//
// VOCAB NOTE: Amm::AddLiquidity has no single root asset/amount — amounts live under
// the variant-discriminated `params` (pooled: tokens[]; concentrated: amountDesired
// {a,b}). `oracle.usd_value` prices ONE (asset, amount) pair, so it CANNOT sum both
// legs (VOCAB §5 projection is scalar-only; no per-leg fan-out here). This manifest
// binds the concentrated lead leg (`params.poolPair.tokenA` + `params.amountDesired.a`)
// as a representative single-leg notional; the true position size may be ~2x. The
// policy reads only the projected USD leaf, so it compiles regardless of how the
// planner resolves the selectors (absent for the pooled variant → policy inert).
// `oracle.usd_value` enriches the lead-leg notional into `context.custom.lpAddUsd`.
// ENRICHMENT — dormant until the /v1/rpc dispatcher serves `oracle.usd_value`.
@id("lp-add-usd-cap")
@severity("warn")
@reason("Large LP add (lead leg over $50k) — confirm the position size before signing")
forbid(principal, action == Amm::Action::"AddLiquidity", resource)
when {
  context has custom
  && context.custom has lpAddUsd
  && context.custom.lpAddUsd.greaterThan(decimal("50000.0000"))
};

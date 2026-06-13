// dambi default v2 policy — explorer id: CURVE-5 (metapool deposit inherits
// hidden base-pool contagion risk).
//
// DERIVED fact (NOT an action-field). A Curve metapool pairs one asset with a
// base-pool LP token (3CRV / crvUSD, …). Depositing silently takes on EVERY
// asset risk of that base pool — if any base coin (USDT/USDC/DAI inside 3CRV)
// depegs, the metapool LP carries it, invisible on screen (2023 USDC depeg
// contaged 3CRV-based metapools). Unlike CURVE-3 (the two assets you can SEE in
// the pool), this is a recursive, one-level-down check: it must (a) decode the
// target metapool to its base pool, then (b) test each base asset's peg band.
// The base-pool composition is partial state (sv = PART — pool metadata is part
// registry feed), and the per-asset peg compare cannot be expressed as a Cedar
// action-field. So the manifest declares `curve.base_pool_depeg` (fold over the
// decoded base pool's assets + peg refs), projecting the Bool
// `context.custom.basePoolDepeg`. Warn when any base asset is out of band.
// Defined-not-wired → `optional` (fail-open no-op).
@id("curve-metapool-base-depeg-warn")
@severity("warn")
@reason("This Curve metapool sits on a base pool that is depegging — you inherit a hidden base-asset risk")
forbid(principal, action == Amm::Action::"AddLiquidity", resource)
when {
  context has custom
  && context.custom has basePoolDepeg
  && context.custom.basePoolDepeg
};

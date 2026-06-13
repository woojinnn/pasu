// dambi default v2 policy — explorer id: UNI-06 (route hops through a thin
// or unknown pool).
//
// A route through a shallow / unknown pool can explode price impact at that hop
// or trap funds in a scam pool. Only the summed `routeEstimatedOut` is exposed
// at the policy layer — the per-hop `PoolState` reserves (route shape: paths ×
// hops) are reducer-internal and too complex for Cedar (sv = LI). The method
// `external.swap_route_min_depth_usd` folds each hop's reserves natively and
// surfaces the route's minimum pool depth (decimal USD) plus a shallow-hop flag.
// `custom` and the call are optional, so each field is `has`-guarded; decimal
// uses `.lessThan(...)` (Cedar has no `<` on decimal). 25,000 USD floor here.
@id("swap-shallow-route-warn")
@severity("warn")
@reason("This swap route passes through a shallow or unknown liquidity pool")
forbid(principal, action == Amm::Action::"Swap", resource)
when {
  context has custom
  && (
    (context.custom has hasShallowHop && context.custom.hasShallowHop)
    || (context.custom has minPoolDepthUsd
        && context.custom.minPoolDepthUsd.lessThan(decimal("25000.0000")))
  )
};

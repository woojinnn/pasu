// dambi default v2 policy — explorer id: UNI-08 (effective fee of the route
// exceeds the user's ceiling).
//
// Uniswap V4 dynamic-fee pools let a hook set the fee per swap, so the fee paid
// is hard to know up front. A V4-specific `PoolKey` dynamic-fee FLAG field is
// not in the current schema; instead the method `external.swap_max_effective_fee_bp`
// takes the max observed effective fee across the route's hops (reducer route
// math) and surfaces it as the enriched `Long` `maxEffectiveFeeBp`. `custom` and
// the call are optional, so the path is `has`-guarded. 100 bp (1%) ceiling here.
@id("swap-uni-v3v4-effective-fee-warn")
@severity("warn")
@reason("The effective fee on this swap route exceeds your fee ceiling")
forbid(principal, action == Amm::Action::"Swap", resource)
when {
  context has custom
  && context.custom has maxEffectiveFeeBp
  && context.custom.maxEffectiveFeeBp > 100
};

// dambi default v2 policy — explorer id: AAVE-04 (supply into a frozen /
// paused market).
//
// ACTION-FIELD. The reserve's governance state flags ride in the supply action's
// live_inputs and are exposed as the base context Bools
// `reserveState.isFrozen` / `reserveState.isPaused`. Supplying into a paused (no
// supply/withdraw/borrow) or frozen (no new borrow) reserve can strand funds with
// no exit — invisible on the wallet screen. Pure read of two base context Bools,
// so the manifest has NO `policy_rpc` and NO `custom_context`.
@id("aave-frozen-paused-supply-deny")
@severity("deny")
@reason("This Aave market is frozen or paused — supplied funds could be stranded")
forbid(principal, action == Lending::Action::"Supply", resource)
when {
  context.reserveState.isFrozen || context.reserveState.isPaused
};

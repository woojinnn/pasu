// dambi default v2 policy — explorer id: AAVE-03 (high-utilization market).
//
// ACTION-FIELD. The reserve's utilization is injected into the borrow action's
// live_inputs and is exposed verbatim as the base context field
// `reserveState.utilizationBp` (Long, basis points). No State₂ recompute and no
// external feed are needed — the policy reads the field directly, so the
// manifest carries NO `policy_rpc` and NO `custom_context` (pure). Warn when the
// market is >95% utilized ('26-04 Aave core markets hit 100% at once → withdrawals
// jammed).
@id("aave-utilization-high-warn")
@severity("warn")
@reason("This Aave market is near full utilization — withdrawals may jam")
forbid(principal, action == Lending::Action::"Borrow", resource)
when {
  context.reserveState.utilizationBp > 9500
};

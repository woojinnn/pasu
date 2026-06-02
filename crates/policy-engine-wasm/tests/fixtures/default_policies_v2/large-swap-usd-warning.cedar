// Default v2 policy — large-swap USD warning. Phase 8B disabled the
// `oracle.usd_value` policy-RPC enrichment that used to populate
// `context.custom.totalInputUsd` (the legacy 8787 dispatcher is being
// decommissioned). The Cedar `has` guard below evaluates to false in
// the absence of the field, so this policy is effectively inert at
// runtime — kept in the fixture set so the manifest / schema /
// per-policy infra tests keep their reference policy.
//
// LiveField-first rewrite (future): once the schema exposes
// `state.holdings[asset].price_usd.value` to Cedar as a decimal-typed
// view, the guard becomes a `state.holdings…multiply(amount).greaterThan
// (decimal("1000.0000"))` expression and the policy regains its
// original intent without any RPC call.
@id("large-swap-usd-warning")
@severity("warn")
forbid(principal, action == Amm::Action::"Swap", resource)
when {
  context has custom
  && context.custom has totalInputUsd
  && context.custom.totalInputUsd.greaterThan(decimal("1000.0000"))
};

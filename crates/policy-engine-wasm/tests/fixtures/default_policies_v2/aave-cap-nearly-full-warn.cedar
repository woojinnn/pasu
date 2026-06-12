// dambi default v2 policy — explorer id: AAVE-12 (supply/borrow cap nearly
// full).
//
// DERIVED fact (NOT pure action-field, despite the raw totals living in
// ReserveState). When a market's borrow/supply cap nears full, withdrawal
// liquidity can dry up and funds get trapped ('26-04 several assets hit 100% at
// once). ReserveState carries totalBorrow / borrowCap only as U256 HEX STRINGS
// (caps optional) — Cedar has no numeric comparison on String, so the used-bp
// ratio cannot be read as an action-field. The manifest declares
// `reserve.cap_used_bp` (cap_kind=borrow), projected into
// `context.custom.borrowCapUsedBp` (Long). Warn above 9500bp (95%).
// Defined-not-wired → `optional` (fail-open).
@id("aave-cap-nearly-full-warn")
@severity("warn")
@reason("This Aave market's borrow cap is nearly full — liquidity may dry up")
forbid(principal, action == Lending::Action::"Borrow", resource)
when {
  context has custom
  && context.custom has borrowCapUsedBp
  && context.custom.borrowCapUsedBp > 9500
};

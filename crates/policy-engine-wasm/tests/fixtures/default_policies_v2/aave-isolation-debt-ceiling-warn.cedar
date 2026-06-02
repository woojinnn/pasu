// scopeball default v2 policy — explorer id: AAVE-11 (isolation-mode debt ceiling
// nearly reached).
//
// PROTOCOL-GLOBAL fact (NOT pure action-field). Isolation assets carry a global
// debt ceiling; once it fills, the tx reverts mid-flight and burns gas. The debt
// ceiling and the protocol-global isolationModeTotalDebt are NOT base context
// fields, so this proximity is derived: the manifest declares
// `reserve.isolation_debt_headroom_bps`, which adds the proposed borrow to the
// global isolation debt and divides by the ceiling, projected into
// `context.custom.isolationDebtUsedBps` (Long basis points). Warn above 9500bp
// (95% of ceiling). Defined-not-wired → `optional` (fail-open).
@id("aave-isolation-debt-ceiling-warn")
@severity("warn")
@reason("This isolated-asset borrow is near the protocol debt ceiling and may revert")
forbid(principal, action == Lending::Action::"Borrow", resource)
when {
  context has custom
  && context.custom has isolationDebtUsedBps
  && context.custom.isolationDebtUsedBps > 9500
};
